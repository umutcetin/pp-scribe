//! Splits a 16 kHz frame stream into utterances using Silero VAD.
//! Shared by the live caption pipeline and the offline (post-meeting) refine pass.

use std::path::Path;

use anyhow::Result;

use super::constants::WHISPER_SAMPLE_RATE;
use super::vad::{SmoothedVad, VadFrame};
use super::{SileroVad, VoiceActivityDetector};

const SR: usize = WHISPER_SAMPLE_RATE as usize;
const VAD_FRAME_SAMPLES: usize = SR * 30 / 1000;

pub struct SegmenterConfig {
    /// Frames of silence (after VAD hangover) that close an utterance.
    pub end_silence_frames: usize,
    /// Utterances are force-cut at this length so captions never lag too far behind.
    pub max_samples: usize,
    /// Emit an interim snapshot every N new samples of an ongoing utterance (0 = never).
    pub interim_every: usize,
}

impl SegmenterConfig {
    pub fn live() -> Self {
        Self {
            end_silence_frames: 8,
            max_samples: SR * 15,
            interim_every: SR * 3 / 2,
        }
    }

    pub fn offline() -> Self {
        Self {
            end_silence_frames: 12,
            max_samples: SR * 28,
            interim_every: 0,
        }
    }
}

pub enum SegmentEvent {
    Interim {
        id: u64,
        start_sample: usize,
        samples: Vec<f32>,
    },
    Final {
        id: u64,
        start_sample: usize,
        samples: Vec<f32>,
    },
}

pub struct Segmenter {
    vad: SmoothedVad,
    cfg: SegmenterConfig,
    pos: usize,
    in_utt: bool,
    next_id: u64,
    cur_id: u64,
    start: usize,
    buf: Vec<f32>,
    silence_run: usize,
    last_interim_len: usize,
}

impl Segmenter {
    pub fn new(vad_model: &Path, cfg: SegmenterConfig) -> Result<Self> {
        let silero = SileroVad::new(vad_model, 0.3)?;
        Ok(Self {
            // prefill 10 frames (300 ms), hangover 10, onset 2 — same shape as pp-cursor
            vad: SmoothedVad::new(Box::new(silero), 10, 10, 2),
            cfg,
            pos: 0,
            in_utt: false,
            next_id: 1,
            cur_id: 0,
            start: 0,
            buf: Vec::new(),
            silence_run: 0,
            last_interim_len: 0,
        })
    }

    /// Feed one frame. Frames that are not exactly 30 ms only advance the clock.
    pub fn push(&mut self, frame: &[f32]) -> Option<SegmentEvent> {
        self.pos += frame.len();
        if frame.len() != VAD_FRAME_SAMPLES {
            return None;
        }

        let decision = self
            .vad
            .push_frame(frame)
            .unwrap_or(VadFrame::Noise);

        match decision {
            VadFrame::Speech(speech) => {
                if !self.in_utt {
                    self.in_utt = true;
                    self.cur_id = self.next_id;
                    self.next_id += 1;
                    self.start = self.pos.saturating_sub(speech.len());
                    self.buf.clear();
                    self.last_interim_len = 0;
                }
                self.buf.extend_from_slice(speech);
                self.silence_run = 0;
            }
            VadFrame::Noise => {
                if self.in_utt {
                    self.silence_run += 1;
                    if self.silence_run >= self.cfg.end_silence_frames {
                        return self.finalize();
                    }
                }
            }
        }

        if self.in_utt && self.buf.len() >= self.cfg.max_samples {
            return self.finalize();
        }

        if self.in_utt
            && self.cfg.interim_every > 0
            && self.buf.len() - self.last_interim_len >= self.cfg.interim_every
        {
            self.last_interim_len = self.buf.len();
            return Some(SegmentEvent::Interim {
                id: self.cur_id,
                start_sample: self.start,
                samples: self.buf.clone(),
            });
        }

        None
    }

    /// Close any open utterance (end of stream).
    pub fn flush(&mut self) -> Option<SegmentEvent> {
        if self.in_utt {
            self.finalize()
        } else {
            None
        }
    }

    fn finalize(&mut self) -> Option<SegmentEvent> {
        self.in_utt = false;
        self.silence_run = 0;
        let samples = std::mem::take(&mut self.buf);
        Some(SegmentEvent::Final {
            id: self.cur_id,
            start_sample: self.start,
            samples,
        })
    }
}
