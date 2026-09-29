//! Thin wrapper over transcribe-rs engines. One `EngineSlot` per role (live / refine)
//! so the fast caption model and the accurate refine model can both stay loaded.

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use transcribe_rs::{
    engines::{
        parakeet::{
            ParakeetEngine, ParakeetInferenceParams, ParakeetModelParams, TimestampGranularity,
        },
        whisper::{WhisperEngine, WhisperInferenceParams},
    },
    TranscriptionEngine,
};

use super::models::{EngineType, ModelManager};

const SR: usize = 16_000;

enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetEngine),
}

#[derive(Debug, Clone)]
pub struct TimedText {
    /// Seconds relative to the start of the audio passed in.
    pub start: f32,
    pub end: f32,
    pub text: String,
}

pub struct EngineSlot {
    models: Arc<ModelManager>,
    inner: Mutex<Option<(String, LoadedEngine)>>,
}

impl EngineSlot {
    pub fn new(models: Arc<ModelManager>) -> Self {
        Self {
            models,
            inner: Mutex::new(None),
        }
    }

    pub fn loaded_model(&self) -> Option<String> {
        self.inner.lock().unwrap().as_ref().map(|(id, _)| id.clone())
    }

    pub fn unload(&self) {
        *self.inner.lock().unwrap() = None;
    }

    /// Loads `model_id` unless it is already the loaded model.
    pub fn ensure(&self, model_id: &str) -> Result<()> {
        let mut guard = self.inner.lock().unwrap();
        if matches!(guard.as_ref(), Some((id, _)) if id == model_id) {
            return Ok(());
        }
        *guard = None;

        let (path, engine_type) = self.models.path(model_id)?;
        let started = std::time::Instant::now();
        let engine = match engine_type {
            EngineType::Whisper => {
                let mut e = WhisperEngine::new();
                e.load_model(&path)
                    .map_err(|e| anyhow!("Failed to load {}: {}", model_id, e))?;
                LoadedEngine::Whisper(e)
            }
            EngineType::Parakeet => {
                let mut e = ParakeetEngine::new();
                e.load_model_with_params(&path, ParakeetModelParams::int8())
                    .map_err(|e| anyhow!("Failed to load {}: {}", model_id, e))?;
                LoadedEngine::Parakeet(e)
            }
            EngineType::Embedding => {
                return Err(anyhow!("{} is not a speech-to-text model", model_id));
            }
        };
        eprintln!("[stt] loaded {} in {:?}", model_id, started.elapsed());
        *guard = Some((model_id.to_string(), engine));
        Ok(())
    }

    /// Transcribes 16 kHz mono samples. Returns full text plus timed segments.
    pub fn transcribe(
        &self,
        model_id: &str,
        mut samples: Vec<f32>,
        language: &str,
        prompt: Option<String>,
    ) -> Result<(String, Vec<TimedText>)> {
        if samples.is_empty() {
            return Ok((String::new(), Vec::new()));
        }
        self.ensure(model_id)?;

        // Both engines do poorly on sub-second clips; pad with silence.
        if samples.len() < SR * 5 / 4 {
            samples.resize(SR * 5 / 4, 0.0);
        }

        let mut guard = self.inner.lock().unwrap();
        let (_, engine) = guard.as_mut().ok_or_else(|| anyhow!("Model not loaded"))?;

        let result = match engine {
            LoadedEngine::Whisper(w) => {
                let params = WhisperInferenceParams {
                    language: (language != "auto").then(|| language.to_string()),
                    initial_prompt: prompt,
                    ..Default::default()
                };
                w.transcribe_samples(samples, Some(params))
            }
            LoadedEngine::Parakeet(p) => {
                let params = ParakeetInferenceParams {
                    timestamp_granularity: TimestampGranularity::Segment,
                    ..Default::default()
                };
                p.transcribe_samples(samples, Some(params))
            }
        }
        .map_err(|e| anyhow!("Transcription failed: {}", e))?;

        let segments = result
            .segments
            .unwrap_or_default()
            .into_iter()
            .map(|s| TimedText {
                start: s.start,
                end: s.end,
                text: s.text.trim().to_string(),
            })
            .filter(|s| !s.text.is_empty())
            .collect();

        Ok((clean_text(&result.text), segments))
    }
}

/// Whisper emits these for silence / noise; they are never real speech in a meeting.
pub fn clean_text(text: &str) -> String {
    let t = text.trim();
    let lowered = t.to_lowercase();
    const NOISE: [&str; 6] = [
        "[blank_audio]",
        "[silence]",
        "(silence)",
        "[music]",
        "[inaudible]",
        "[ Silence ]",
    ];
    if NOISE.iter().any(|n| lowered == n.to_lowercase()) {
        return String::new();
    }
    t.to_string()
}
