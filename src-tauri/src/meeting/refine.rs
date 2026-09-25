//! Post-meeting pass: re-segments each recording offline with VAD, groups speech into
//! ≤28 s chunks, and transcribes them with the accurate model. Working per speaker file
//! keeps attribution exact and skipping silence avoids Whisper hallucinations.

use std::path::Path;

use anyhow::{anyhow, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::storage::{self, Segment, Speaker};
use crate::audio_toolkit::segmenter::SegmenterConfig;
use crate::audio_toolkit::{SegmentEvent, Segmenter};
use crate::stt::EngineSlot;

const SR: usize = 16_000;
const FRAME: usize = SR * 30 / 1000;
const MERGE_GAP: usize = SR * 6 / 5;
const MAX_CHUNK: usize = SR * 28;
const PAD: usize = SR * 3 / 20;

#[derive(Clone, Serialize)]
pub struct RefineProgress {
    pub meeting_id: String,
    pub done: usize,
    pub total: usize,
    pub stage: String,
}

struct Chunk {
    speaker: Speaker,
    start: usize,
    end: usize,
}

pub fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate as usize != SR || spec.channels != 1 {
        return Err(anyhow!("Unexpected WAV format in {:?}", path));
    }
    Ok(reader
        .samples::<i16>()
        .map_while(|s| s.ok())
        .map(|s| s as f32 / i16::MAX as f32)
        .collect())
}

pub fn speech_regions(samples: &[f32], vad_model: &Path) -> Result<Vec<(usize, usize)>> {
    let mut seg = Segmenter::new(vad_model, SegmenterConfig::offline())?;
    let mut regions = Vec::new();
    let mut collect = |ev: Option<SegmentEvent>| {
        if let Some(SegmentEvent::Final { start_sample, samples, .. }) = ev {
            regions.push((start_sample, start_sample + samples.len()));
        }
    };
    for frame in samples.chunks(FRAME) {
        collect(seg.push(frame));
    }
    collect(seg.flush());
    Ok(regions)
}

fn merge(speaker: Speaker, regions: Vec<(usize, usize)>) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    for (start, end) in regions {
        if let Some(last) = chunks.last_mut() {
            if start.saturating_sub(last.end) <= MERGE_GAP && end - last.start <= MAX_CHUNK {
                last.end = end;
                continue;
            }
        }
        chunks.push(Chunk { speaker, start, end });
    }
    chunks
}

pub struct RefineJob<'a> {
    pub app: &'a AppHandle,
    pub meeting_id: &'a str,
    pub dir: &'a Path,
    pub vad_model: &'a Path,
    pub slot: &'a EngineSlot,
    pub model: &'a str,
    pub language: &'a str,
}

pub fn run(job: RefineJob) -> Result<Vec<Segment>> {
    let progress = |done: usize, total: usize, stage: &str| {
        let _ = job.app.emit(
            "refine-progress",
            RefineProgress {
                meeting_id: job.meeting_id.to_string(),
                done,
                total,
                stage: stage.to_string(),
            },
        );
    };

    progress(0, 0, "Loading model");
    job.slot.ensure(job.model)?;

    progress(0, 0, "Finding speech");
    let mut tracks: Vec<(Speaker, Vec<f32>)> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    for speaker in [Speaker::Me, Speaker::Them] {
        let path = storage::wav_path(job.dir, speaker);
        if !path.exists() {
            continue;
        }
        let samples = read_wav(&path)?;
        chunks.extend(merge(speaker, speech_regions(&samples, job.vad_model)?));
        tracks.push((speaker, samples));
    }
    // Interleave speakers chronologically so progress reads naturally.
    chunks.sort_by_key(|c| c.start);

    let total = chunks.len();
    let mut out: Vec<Segment> = Vec::new();
    let mut prev_text: [String; 2] = Default::default();

    for (i, c) in chunks.iter().enumerate() {
        progress(i, total, "Transcribing");
        let samples = &tracks.iter().find(|(s, _)| *s == c.speaker).unwrap().1;
        let from = c.start.saturating_sub(PAD);
        let to = (c.end + PAD).min(samples.len());
        if to <= from {
            continue;
        }

        // A little of the speaker's previous text helps Whisper with names and terms.
        let idx = c.speaker as usize;
        let prompt = (!prev_text[idx].is_empty()).then(|| tail(&prev_text[idx], 200));

        let (text, timed) =
            job.slot
                .transcribe(job.model, samples[from..to].to_vec(), job.language, prompt)?;
        if text.is_empty() {
            continue;
        }
        prev_text[idx] = text.clone();

        let base_ms = (from * 1000 / SR) as u64;
        let chunk_end_ms = (to * 1000 / SR) as u64;
        if timed.is_empty() {
            out.push(Segment {
                id: format!("{}-r{}", c.speaker.key(), i),
                speaker: c.speaker,
                start_ms: (c.start * 1000 / SR) as u64,
                end_ms: (c.end * 1000 / SR) as u64,
                text,
            });
        } else {
            for (k, t) in timed.into_iter().enumerate() {
                out.push(Segment {
                    id: format!("{}-r{}-{}", c.speaker.key(), i, k),
                    speaker: c.speaker,
                    start_ms: base_ms + (t.start * 1000.0) as u64,
                    end_ms: (base_ms + (t.end * 1000.0) as u64).min(chunk_end_ms),
                    text: t.text,
                });
            }
        }
    }

    out.sort_by_key(|s| s.start_ms);
    progress(total, total, "Done");
    Ok(out)
}

fn tail(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    s.chars().skip(n.saturating_sub(max_chars)).collect()
}
