//! Model catalog, on-disk lookup and downloads.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EngineType {
    Whisper,
    Parakeet,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_mb: u64,
    pub engine: EngineType,
    /// Parakeet is English only.
    pub multilingual: bool,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    #[serde(skip)]
    filename: String,
    #[serde(skip)]
    url: String,
    #[serde(skip)]
    is_archive: bool,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    model_id: String,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
}

pub struct ModelManager {
    app: AppHandle,
    models_dir: PathBuf,
    models: Mutex<HashMap<String, ModelInfo>>,
}

fn catalog() -> Vec<ModelInfo> {
    let m = |id: &str,
             name: &str,
             description: &str,
             size_mb: u64,
             engine: EngineType,
             multilingual: bool,
             filename: &str,
             url: &str,
             is_archive: bool| ModelInfo {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        size_mb,
        engine,
        multilingual,
        is_downloaded: false,
        is_downloading: false,
        filename: filename.into(),
        url: url.into(),
        is_archive,
    };
    vec![
        m(
            "parakeet-v2",
            "Parakeet V2",
            "English only. Very fast — best for live captions.",
            473,
            EngineType::Parakeet,
            false,
            "parakeet-tdt-0.6b-v2-int8",
            "https://blob.handy.computer/parakeet-v2-int8.tar.gz",
            true,
        ),
        m(
            "whisper-small",
            "Whisper Small",
            "Multilingual, fast. Live captions for non-English meetings.",
            487,
            EngineType::Whisper,
            true,
            "ggml-small.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
            false,
        ),
        m(
            "whisper-large-v3-turbo",
            "Whisper Large v3 Turbo",
            "Multilingual, most accurate. Best for the post-meeting transcript.",
            874,
            EngineType::Whisper,
            true,
            "ggml-large-v3-turbo-q8_0.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin",
            false,
        ),
    ]
}

impl ModelManager {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let models_dir = app.path().app_data_dir()?.join("models");
        fs::create_dir_all(&models_dir)?;

        let models = catalog().into_iter().map(|m| (m.id.clone(), m)).collect();
        let manager = Self {
            app: app.clone(),
            models_dir,
            models: Mutex::new(models),
        };
        manager.refresh();
        Ok(manager)
    }

    /// Model folders of sibling apps (pp-cursor / Handy) — reused so nothing is downloaded twice.
    fn search_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.models_dir.clone()];
        if let Some(support) = self.models_dir.parent().and_then(|p| p.parent()) {
            for id in ["com.handyaipair.app", "com.pais.handy", "com.ppmeeting.app"] {
                dirs.push(support.join(id).join("models"));
            }
        }
        dirs
    }

    fn locate(&self, filename: &str) -> Option<PathBuf> {
        self.search_dirs()
            .into_iter()
            .map(|d| d.join(filename))
            .find(|p| p.exists())
    }

    fn refresh(&self) {
        let mut models = self.models.lock().unwrap();
        for m in models.values_mut() {
            m.is_downloaded = self.locate(&m.filename).is_some();
        }
    }

    pub fn list(&self) -> Vec<ModelInfo> {
        let models = self.models.lock().unwrap();
        let mut list: Vec<_> = models.values().cloned().collect();
        list.sort_by_key(|m| m.size_mb);
        list
    }

    pub fn info(&self, id: &str) -> Option<ModelInfo> {
        self.models.lock().unwrap().get(id).cloned()
    }

    pub fn path(&self, id: &str) -> Result<(PathBuf, EngineType)> {
        let info = self.info(id).ok_or_else(|| anyhow!("Unknown model: {}", id))?;
        let path = self
            .locate(&info.filename)
            .ok_or_else(|| anyhow!("Model '{}' is not downloaded", info.name))?;
        Ok((path, info.engine))
    }

    fn set_downloading(&self, id: &str, value: bool) {
        if let Some(m) = self.models.lock().unwrap().get_mut(id) {
            m.is_downloading = value;
        }
    }

    pub async fn download(&self, id: &str) -> Result<()> {
        let info = self.info(id).ok_or_else(|| anyhow!("Unknown model: {}", id))?;
        if info.is_downloading {
            return Err(anyhow!("Already downloading"));
        }
        self.set_downloading(id, true);
        let result = self.download_inner(&info).await;
        self.set_downloading(id, false);
        self.refresh();

        let _ = self.app.emit(
            "model-download-progress",
            DownloadProgress {
                model_id: id.into(),
                downloaded: 0,
                total: 0,
                done: true,
                error: result.as_ref().err().map(|e| e.to_string()),
            },
        );
        result
    }

    async fn download_inner(&self, info: &ModelInfo) -> Result<()> {
        let partial = self.models_dir.join(format!("{}.partial", info.filename));
        let response = reqwest::get(&info.url).await?.error_for_status()?;
        let total = response.content_length().unwrap_or(info.size_mb * 1024 * 1024);

        let mut file = tokio::fs::File::create(&partial).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut last_emit: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if downloaded - last_emit > 4 * 1024 * 1024 {
                last_emit = downloaded;
                let _ = self.app.emit(
                    "model-download-progress",
                    DownloadProgress {
                        model_id: info.id.clone(),
                        downloaded,
                        total,
                        done: false,
                        error: None,
                    },
                );
            }
        }
        file.flush().await?;
        drop(file);

        if info.is_archive {
            let dir = self.models_dir.clone();
            let archive = partial.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let tar_gz = fs::File::open(&archive)?;
                tar::Archive::new(flate2::read::GzDecoder::new(tar_gz)).unpack(&dir)?;
                Ok(())
            })
            .await??;
            fs::remove_file(&partial)?;
        } else {
            fs::rename(&partial, self.models_dir.join(&info.filename))?;
        }
        Ok(())
    }
}
