//! Headless check of the audio + STT pipeline (no Tauri window).
//!   cargo run --example smoke -- capture   # system-audio + mic capture, live-style segmentation, Parakeet
//!   cargo run --example smoke -- refine <file.wav>   # offline VAD + chunked Whisper
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use pp_scribe_lib::audio_toolkit::segmenter::SegmenterConfig;
use pp_scribe_lib::audio_toolkit::{AudioCapture, CaptureSource, SegmentEvent, Segmenter};
use transcribe_rs::engines::parakeet::{ParakeetEngine, ParakeetModelParams};
use transcribe_rs::engines::whisper::{WhisperEngine, WhisperInferenceParams};
use transcribe_rs::TranscriptionEngine;

fn support() -> PathBuf {
    dirs_home().join("Library/Application Support/com.handyaipair.app/models")
}
fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap())
}
fn vad() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/models/silero_vad_v4.onnx")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("refine") => refine(&args[2]),
        _ => capture(),
    }
}

fn capture() {
    let mut engine = ParakeetEngine::new();
    engine
        .load_model_with_params(&support().join("parakeet-tdt-0.6b-v2-int8"), ParakeetModelParams::int8())
        .unwrap();
    let engine = Arc::new(Mutex::new(engine));

    let (tx, rx) = mpsc::channel::<(String, SegmentEvent)>();
    let mut caps = Vec::new();
    for (name, src) in [("them", CaptureSource::SystemOutput), ("me", CaptureSource::Input(None))] {
        let tx = tx.clone();
        let mut seg = Segmenter::new(&vad(), SegmenterConfig::live()).unwrap();
        let mut total = 0usize;
        let mut peak = 0f32;
        let name_s = name.to_string();
        let cap = AudioCapture::start(src, move |f| {
            total += f.len();
            peak = f.iter().fold(peak, |m, s| m.max(s.abs()));
            if total % (16000 * 2) < f.len() {
                eprintln!("  [{}] {:.1}s captured, peak {:.3}", name_s, total as f32 / 16000.0, peak);
                peak = 0.0;
            }
            if let Some(ev) = seg.push(f) {
                let _ = tx.send((name_s.clone(), ev));
            }
        });
        match cap {
            Ok(c) => {
                println!("{} -> {}", name, c.device_name);
                caps.push(c);
            }
            Err(e) => println!("{} FAILED: {}", name, e),
        }
    }
    drop(tx);

    std::thread::sleep(Duration::from_millis(500));
    let say = std::process::Command::new("say")
        .args(["-r", "185", "Hello everyone, thanks for joining. Today we will review the quarterly roadmap and discuss the migration to the new billing system."])
        .spawn()
        .unwrap();
    let mut say = say;
    say.wait().unwrap();
    std::thread::sleep(Duration::from_secs(2));
    for c in caps {
        c.stop();
    }

    for (name, ev) in rx.iter() {
        let (kind, start, samples) = match ev {
            SegmentEvent::Interim { start_sample, samples, .. } => ("interim", start_sample, samples),
            SegmentEvent::Final { start_sample, samples, .. } => ("FINAL", start_sample, samples),
        };
        let secs = samples.len() as f32 / 16000.0;
        let t = std::time::Instant::now();
        let text = engine.lock().unwrap().transcribe_samples(samples, None).unwrap().text;
        println!(
            "[{} {} @{:.2}s len {:.2}s, stt {:?}] {}",
            name, kind, start as f32 / 16000.0, secs, t.elapsed(), text
        );
    }
}

fn refine(path: &str) {
    let samples = pp_scribe_lib::meeting::refine::read_wav(std::path::Path::new(path)).unwrap();
    let regions = pp_scribe_lib::meeting::refine::speech_regions(&samples, &vad()).unwrap();
    println!("{} samples, regions: {:?}", samples.len(), regions);
    let mut w = WhisperEngine::new();
    w.load_model(&support().join("ggml-small.bin")).unwrap();
    for (s, e) in regions {
        let r = w
            .transcribe_samples(samples[s..e].to_vec(), Some(WhisperInferenceParams { language: Some("en".into()), ..Default::default() }))
            .unwrap();
        println!("[{:.2}-{:.2}] {}", s as f32 / 16000.0, e as f32 / 16000.0, r.text);
    }
}
