use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::{anyhow, Result};
use chrono::Local;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::live::{spawn_caption_worker, CaptionWorkerCtx, ChannelPipeline, Job};
use super::refine::{self, RefineJob};
use super::storage::{self, MeetingMeta, MeetingStatus, Segment, Speaker};
use crate::audio_toolkit::{AudioCapture, CaptureSource};
use crate::settings::{get_settings, ThemSource};
use crate::stt::{EngineSlot, ModelManager};

struct ActiveMeeting {
    meta: MeetingMeta,
    dir: PathBuf,
    started: Instant,
    captures: Vec<AudioCapture>,
    worker: JoinHandle<()>,
    captions: Arc<Mutex<Vec<Segment>>>,
}

#[derive(Serialize, Clone)]
pub struct ActiveSnapshot {
    pub meta: MeetingMeta,
    pub elapsed_ms: u64,
    pub captions: Vec<Segment>,
}

#[derive(Serialize, Clone)]
struct MeetingStateEvent {
    state: &'static str,
    meta: Option<MeetingMeta>,
}

pub struct MeetingManager {
    app: AppHandle,
    pub models: Arc<ModelManager>,
    live_slot: Arc<EngineSlot>,
    refine_slot: Arc<EngineSlot>,
    active: Mutex<Option<ActiveMeeting>>,
    refining: Arc<AtomicBool>,
}

impl MeetingManager {
    pub fn new(app: &AppHandle, models: Arc<ModelManager>) -> Self {
        Self {
            app: app.clone(),
            live_slot: Arc::new(EngineSlot::new(models.clone())),
            refine_slot: Arc::new(EngineSlot::new(models.clone())),
            models,
            active: Mutex::new(None),
            refining: Arc::new(AtomicBool::new(false)),
        }
    }

    fn vad_model(&self) -> Result<PathBuf> {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/models/silero_vad_v4.onnx");
        if dev.exists() {
            return Ok(dev);
        }
        Ok(self.app.path().resolve(
            "resources/models/silero_vad_v4.onnx",
            tauri::path::BaseDirectory::Resource,
        )?)
    }

    fn kb(&self) -> Option<Arc<crate::knowledge::KnowledgeManager>> {
        self.app
            .try_state::<Arc<crate::knowledge::KnowledgeManager>>()
            .map(|s| s.inner().clone())
    }

    /// Warm the live model so the first caption isn't delayed by model loading.
    pub fn preload_live_model(self: &Arc<Self>) {
        let this = self.clone();
        std::thread::spawn(move || {
            let model = get_settings(&this.app).live_model;
            if let Err(e) = this.live_slot.ensure(&model) {
                eprintln!("[stt] preload skipped: {}", e);
            }
        });
    }

    pub fn active_id(&self) -> Option<String> {
        self.active.lock().unwrap().as_ref().map(|a| a.meta.id.clone())
    }

    pub fn snapshot(&self) -> Option<ActiveSnapshot> {
        self.active.lock().unwrap().as_ref().map(|a| ActiveSnapshot {
            meta: a.meta.clone(),
            elapsed_ms: a.started.elapsed().as_millis() as u64,
            captions: a.captions.lock().unwrap().clone(),
        })
    }

    pub fn start(&self, title: Option<String>) -> Result<MeetingMeta> {
        let mut active = self.active.lock().unwrap();
        if active.is_some() {
            return Err(anyhow!("A meeting is already being recorded"));
        }

        let settings = get_settings(&self.app);
        self.models.path(&settings.live_model)?; // fail fast if the live model is missing
        let vad_model = self.vad_model()?;

        let started_at = Local::now();
        let (id, dir) = storage::create_meeting_dir(&self.app, &started_at)?;
        let title = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| format!("Meeting {}", started_at.format("%b %-d, %H:%M")));

        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let captions = Arc::new(Mutex::new(Vec::new()));
        let worker = spawn_caption_worker(
            CaptionWorkerCtx {
                app: self.app.clone(),
                meeting_id: id.clone(),
                dir: dir.clone(),
                slot: self.live_slot.clone(),
                model: settings.live_model.clone(),
                language: settings.language.clone(),
                captions: captions.clone(),
            },
            job_rx,
        );

        let started = Instant::now();
        let mut captures = Vec::new();
        let mut warnings = Vec::new();

        let mut sources = vec![(Speaker::Me, CaptureSource::Input(settings.mic_device.clone()))];
        match settings.them_source {
            ThemSource::System => sources.push((Speaker::Them, CaptureSource::SystemOutput)),
            ThemSource::Device => match &settings.them_device {
                Some(name) => sources.push((Speaker::Them, CaptureSource::Input(Some(name.clone())))),
                None => warnings.push("No device selected for the other side — only your mic is recorded.".to_string()),
            },
            ThemSource::Off => {}
        }

        let mut me_device = None;
        let mut them_device = None;
        for (speaker, source) in sources {
            let mut pipeline =
                ChannelPipeline::new(&self.app, speaker, &dir, &vad_model, started, job_tx.clone())?;
            match AudioCapture::start(source, move |frame| pipeline.on_frame(frame)) {
                Ok(cap) => {
                    match speaker {
                        Speaker::Me => me_device = Some(cap.device_name.clone()),
                        Speaker::Them => them_device = Some(cap.device_name.clone()),
                    }
                    captures.push(cap);
                }
                Err(e) => {
                    let _ = std::fs::remove_file(storage::wav_path(&dir, speaker));
                    warnings.push(format!(
                        "Could not capture {}: {}",
                        match speaker {
                            Speaker::Me => "your microphone",
                            Speaker::Them => "system audio",
                        },
                        e
                    ));
                }
            }
        }
        drop(job_tx); // pipelines own the remaining senders

        if captures.is_empty() {
            let _ = worker.join();
            let _ = std::fs::remove_dir_all(&dir);
            return Err(anyhow!(warnings.join("\n")));
        }
        for w in warnings {
            let _ = self.app.emit("meeting-warning", w);
        }

        let meta = MeetingMeta {
            id,
            title,
            started_at,
            ended_at: None,
            duration_ms: 0,
            status: MeetingStatus::Recording,
            language: settings.language.clone(),
            live_model: settings.live_model.clone(),
            refine_model: None,
            refine_error: None,
            me_label: settings.me_label.clone(),
            them_label: settings.them_label.clone(),
            me_device,
            them_device,
        };
        storage::write_meta(&dir, &meta)?;

        *active = Some(ActiveMeeting {
            meta: meta.clone(),
            dir,
            started,
            captures,
            worker,
            captions,
        });
        let _ = self.app.emit(
            "meeting-state",
            MeetingStateEvent { state: "recording", meta: Some(meta.clone()) },
        );
        Ok(meta)
    }

    pub fn rename_active(&self, title: &str) -> Result<()> {
        let mut guard = self.active.lock().unwrap();
        let a = guard.as_mut().ok_or_else(|| anyhow!("No active meeting"))?;
        a.meta.title = title.trim().to_string();
        storage::write_meta(&a.dir, &a.meta)
    }

    /// Stops capture, waits for the remaining captions, and (optionally) kicks off refine.
    pub fn stop(self: &Arc<Self>) -> Result<MeetingMeta> {
        let a = self
            .active
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| anyhow!("No active meeting"))?;
        let _ = self.app.emit(
            "meeting-state",
            MeetingStateEvent { state: "finalizing", meta: Some(a.meta.clone()) },
        );

        let duration_ms = a.started.elapsed().as_millis() as u64;
        for cap in a.captures {
            cap.stop(); // drops the pipeline: flushes last utterance + finalizes WAV
        }
        let _ = a.worker.join();

        let mut meta = a.meta;
        meta.ended_at = Some(Local::now());
        meta.duration_ms = duration_ms;
        meta.status = MeetingStatus::Recorded;
        storage::write_meta(&a.dir, &meta)?;
        let _ = storage::export_markdown(&self.app, &meta.id);

        let _ = self.app.emit(
            "meeting-state",
            MeetingStateEvent { state: "stopped", meta: Some(meta.clone()) },
        );
        // Live captions are searchable right away; refine replaces them later.
        if let Some(kb) = self.kb() {
            kb.ingest_meeting(&meta.id);
        }

        if get_settings(&self.app).auto_refine {
            if let Err(e) = self.refine(&meta.id) {
                let _ = self.app.emit("meeting-warning", format!("Refine not started: {}", e));
            }
        }
        Ok(meta)
    }

    pub fn is_refining(&self) -> bool {
        self.refining.load(Ordering::Relaxed)
    }

    /// Runs the accurate transcription in the background. One refine at a time.
    pub fn refine(self: &Arc<Self>, id: &str) -> Result<()> {
        if self.active_id().as_deref() == Some(id) {
            return Err(anyhow!("Stop the meeting before refining it"));
        }
        let settings = get_settings(&self.app);
        self.models.path(&settings.refine_model)?;
        if self.refining.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("Another transcript is already being refined"));
        }

        let dir = storage::meeting_dir(&self.app, id)?;
        let mut meta = storage::read_meta(&dir)?;
        meta.status = MeetingStatus::Refining;
        meta.refine_error = None;
        storage::write_meta(&dir, &meta)?;
        let _ = self.app.emit("meeting-updated", meta.clone());

        let this = self.clone();
        let vad_model = self.vad_model()?;
        std::thread::spawn(move || {
            let started = Instant::now();
            let result = refine::run(RefineJob {
                app: &this.app,
                meeting_id: &meta.id,
                dir: &dir,
                vad_model: &vad_model,
                slot: &this.refine_slot,
                model: &settings.refine_model,
                language: &settings.language,
            });
            // Free ~1 GB between meetings; reloading takes a few seconds.
            this.refine_slot.unload();

            match result {
                Ok(segs) => {
                    let _ = storage::write_refined(&dir, &segs);
                    meta.status = MeetingStatus::Refined;
                    meta.refine_model = Some(settings.refine_model.clone());
                    eprintln!("[refine] {} segments in {:?}", segs.len(), started.elapsed());
                }
                Err(e) => {
                    eprintln!("[refine] failed: {}", e);
                    meta.status = MeetingStatus::RefineFailed;
                    meta.refine_error = Some(e.to_string());
                }
            }
            let _ = storage::write_meta(&dir, &meta);
            let _ = storage::export_markdown(&this.app, &meta.id);
            if let Some(kb) = this.kb() {
                kb.ingest_meeting(&meta.id);
            }
            this.refining.store(false, Ordering::SeqCst);
            let _ = this.app.emit("meeting-updated", meta);
        });
        Ok(())
    }

    /// Reload the live model after settings change (only between meetings).
    pub fn on_settings_changed(self: &Arc<Self>) {
        if self.active_id().is_none() {
            self.live_slot.unload();
            self.preload_live_model();
        }
    }
}
