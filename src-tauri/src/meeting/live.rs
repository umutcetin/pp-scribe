//! Live pipeline. Each channel (me / them) runs on its capture thread:
//!   frame -> WAV file (full recording) -> VAD segmenter -> job queue
//! A single caption worker drains the queue with the fast live model and emits `caption` events.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::storage::{self, Segment, Speaker};
use crate::audio_toolkit::segmenter::SegmenterConfig;
use crate::audio_toolkit::{SegmentEvent, Segmenter};
use crate::stt::EngineSlot;

const SR: usize = 16_000;
const FRAME: usize = SR * 30 / 1000;
/// If a device delivers nothing for this long (e.g. an idle system-audio tap), pad with
/// silence so both recordings stay aligned with the wall clock.
const MAX_DRIFT: usize = SR / 2;
const WAV_FLUSH_EVERY: usize = SR * 5;
const LEVEL_EVERY: usize = SR / 10;

pub struct Job {
    pub speaker: Speaker,
    pub utt_id: u64,
    pub start_sample: usize,
    pub samples: Vec<f32>,
    pub is_final: bool,
}

#[derive(Clone, Serialize)]
pub struct CaptionEvent {
    pub meeting_id: String,
    pub id: String,
    pub speaker: Speaker,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub is_final: bool,
}

#[derive(Clone, Serialize)]
struct LevelEvent {
    speaker: Speaker,
    level: f32,
}

pub struct ChannelPipeline {
    app: AppHandle,
    speaker: Speaker,
    started: Instant,
    wav: Option<hound::WavWriter<BufWriter<File>>>,
    segmenter: Segmenter,
    jobs: mpsc::Sender<Job>,
    written: usize,
    since_flush: usize,
    level_acc: f32,
    level_n: usize,
}

impl ChannelPipeline {
    pub fn new(
        app: &AppHandle,
        speaker: Speaker,
        dir: &Path,
        vad_model: &Path,
        started: Instant,
        jobs: mpsc::Sender<Job>,
    ) -> Result<Self> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SR as u32,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let wav = hound::WavWriter::create(storage::wav_path(dir, speaker), spec)?;
        Ok(Self {
            app: app.clone(),
            speaker,
            started,
            wav: Some(wav),
            segmenter: Segmenter::new(vad_model, SegmenterConfig::live())?,
            jobs,
            written: 0,
            since_flush: 0,
            level_acc: 0.0,
            level_n: 0,
        })
    }

    pub fn on_frame(&mut self, frame: &[f32]) {
        let expected = (self.started.elapsed().as_secs_f64() * SR as f64) as usize;
        if expected > self.written + frame.len() + MAX_DRIFT {
            let gap = expected - self.written - frame.len();
            let silence = [0.0f32; FRAME];
            let mut left = gap;
            while left > 0 {
                let n = left.min(FRAME);
                self.process(&silence[..n], false);
                left -= n;
            }
        }
        self.process(frame, true);
    }

    fn process(&mut self, frame: &[f32], real: bool) {
        if let Some(wav) = self.wav.as_mut() {
            for &s in frame {
                let _ = wav.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            }
        }
        self.written += frame.len();
        self.since_flush += frame.len();
        if self.since_flush >= WAV_FLUSH_EVERY {
            self.since_flush = 0;
            // keeps the WAV header valid if the app dies mid-meeting
            if let Some(wav) = self.wav.as_mut() {
                let _ = wav.flush();
            }
        }

        if real {
            let peak = frame.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            self.level_acc = self.level_acc.max(peak);
            self.level_n += frame.len();
            if self.level_n >= LEVEL_EVERY {
                let _ = self.app.emit(
                    "audio-level",
                    LevelEvent {
                        speaker: self.speaker,
                        level: self.level_acc,
                    },
                );
                self.level_acc = 0.0;
                self.level_n = 0;
            }
        }

        if let Some(ev) = self.segmenter.push(frame) {
            self.send(ev);
        }
    }

    fn send(&self, ev: SegmentEvent) {
        let job = match ev {
            SegmentEvent::Interim { id, start_sample, samples } => Job {
                speaker: self.speaker,
                utt_id: id,
                start_sample,
                samples,
                is_final: false,
            },
            SegmentEvent::Final { id, start_sample, samples } => Job {
                speaker: self.speaker,
                utt_id: id,
                start_sample,
                samples,
                is_final: true,
            },
        };
        let _ = self.jobs.send(job);
    }
}

impl Drop for ChannelPipeline {
    /// Runs on the capture thread after the last frame: close the open utterance and the WAV.
    fn drop(&mut self) {
        if let Some(ev) = self.segmenter.flush() {
            self.send(ev);
        }
        if let Some(wav) = self.wav.take() {
            if let Err(e) = wav.finalize() {
                eprintln!("[live] failed to finalize wav: {}", e);
            }
        }
    }
}

pub struct CaptionWorkerCtx {
    pub app: AppHandle,
    pub meeting_id: String,
    pub dir: PathBuf,
    pub slot: Arc<EngineSlot>,
    pub model: String,
    pub language: String,
    pub captions: Arc<Mutex<Vec<Segment>>>,
}

pub fn spawn_caption_worker(ctx: CaptionWorkerCtx, rx: mpsc::Receiver<Job>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        if let Err(e) = ctx.slot.ensure(&ctx.model) {
            let _ = ctx.app.emit("meeting-warning", format!("Live model failed to load: {}", e));
        }

        while let Ok(first) = rx.recv() {
            let mut batch = vec![first];
            while let Ok(j) = rx.try_recv() {
                batch.push(j);
            }

            // Interims are only a preview: keep the newest per utterance, and none at all
            // for utterances whose final text is already in this batch.
            let (finals, pending): (Vec<Job>, Vec<Job>) =
                batch.into_iter().partition(|j| j.is_final);
            let mut interims: HashMap<(Speaker, u64), Job> = HashMap::new();
            for j in pending {
                if !finals.iter().any(|f| f.speaker == j.speaker && f.utt_id == j.utt_id) {
                    interims.insert((j.speaker, j.utt_id), j);
                }
            }

            for job in finals.into_iter().chain(interims.into_values()) {
                handle_job(&ctx, job);
            }
        }
    })
}

fn handle_job(ctx: &CaptionWorkerCtx, job: Job) {
    let start_ms = (job.start_sample * 1000 / SR) as u64;
    let end_ms = ((job.start_sample + job.samples.len()) * 1000 / SR) as u64;
    let id = format!("{}-{}", job.speaker.key(), job.utt_id);

    let text = match ctx.slot.transcribe(&ctx.model, job.samples, &ctx.language, None) {
        Ok((text, _)) => text,
        Err(e) => {
            eprintln!("[live] transcription error: {}", e);
            String::new()
        }
    };

    if job.is_final && !text.is_empty() {
        let seg = Segment {
            id: id.clone(),
            speaker: job.speaker,
            start_ms,
            end_ms,
            text: text.clone(),
        };
        if let Err(e) = storage::append_live(&ctx.dir, &seg) {
            eprintln!("[live] failed to save caption: {}", e);
        }
        ctx.captions.lock().unwrap().push(seg);
    }

    // An empty final still gets emitted so the UI can drop the interim bubble.
    let _ = ctx.app.emit(
        "caption",
        CaptionEvent {
            meeting_id: ctx.meeting_id.clone(),
            id,
            speaker: job.speaker,
            start_ms,
            end_ms,
            text,
            is_final: job.is_final,
        },
    );
}
