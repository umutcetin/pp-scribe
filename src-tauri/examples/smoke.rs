//! Headless check of the audio + STT pipeline (no Tauri window).
//!   cargo run --example smoke -- capture ["BlackHole 2ch"]   # them (system tap or loopback device) + mic, live segmentation, Parakeet
//!   cargo run --example smoke -- refine <file.wav>   # offline VAD + chunked Whisper
//!   cargo run --example smoke -- kb <db> <file|url|meeting-dir>... -- <query>...   # ingest + hybrid search
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
        Some("kb") => kb(&args[2..]),
        _ => capture(args.get(2).cloned()),
    }
}

/// `them_device`: capture that input device (e.g. "BlackHole 2ch") instead of the system tap,
/// and play the test sentence straight into it so nothing is audible.
fn capture(them_device: Option<String>) {
    let mut engine = ParakeetEngine::new();
    engine
        .load_model_with_params(&support().join("parakeet-tdt-0.6b-v2-int8"), ParakeetModelParams::int8())
        .unwrap();
    let engine = Arc::new(Mutex::new(engine));

    let (tx, rx) = mpsc::channel::<(String, SegmentEvent)>();
    let mut caps = Vec::new();
    let them_src = match &them_device {
        Some(d) => CaptureSource::Input(Some(d.clone())),
        None => CaptureSource::SystemOutput,
    };
    for (name, src) in [("them", them_src), ("me", CaptureSource::Input(None))] {
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
    let mut say_args = vec!["-r", "185"];
    if let Some(d) = &them_device {
        say_args.extend(["-a", d.as_str()]);
    }
    let say = std::process::Command::new("say")
        .args(say_args)
        .arg("Hello everyone, thanks for joining. Today we will review the quarterly roadmap and discuss the migration to the new billing system.")
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

fn kb(args: &[String]) {
    use pp_scribe_lib::knowledge::{chunk, embed::Embedder, extract, search, store};
    use pp_scribe_lib::meeting::storage;

    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let (db, inputs, queries) = (&args[0], &args[1..split], &args[(split + 1).min(args.len())..]);
    let _ = std::fs::remove_file(db);
    let st = store::Store::open(std::path::Path::new(db), "smoke").unwrap();
    use pp_scribe_lib::knowledge::embed;
    let (dir, spec) = match std::env::var("EMBED").as_deref() {
        Ok("e5") => ("multilingual-e5-base", embed::E5),
        _ => ("bge-m3", embed::BGE_M3),
    };
    let t = std::time::Instant::now();
    let embedder = Embedder::load(&dirs_home().join("Library/Application Support/com.ppscribe.app/models").join(dir), spec).unwrap();
    println!("{dir} loaded in {:?}", t.elapsed());

    for input in inputs {
        let p = std::path::Path::new(input);
        let ex = if input.starts_with("http") {
            match tauri::async_runtime::block_on(extract::url::fetch(input)).unwrap() {
                extract::url::Fetched::Page(ex) => ex,
                extract::url::Fetched::Pdf(_) => panic!("pdf url"),
            }
        } else if p.join("meta.json").exists() {
            extract::meeting::from_detail(storage::MeetingDetail {
                meta: storage::read_meta(p).unwrap(),
                live: storage::read_live(p),
                refined: storage::read_refined(p),
                dir: input.clone(),
            })
        } else {
            match p.extension().and_then(|e| e.to_str()) {
                Some("pdf") => {
                    let r = extract::pdf::extract(p).unwrap();
                    println!("  pdf pages {} (no text: {})", r.pages, r.pages_without_text);
                    r.extracted
                }
                Some("docx") => extract::docx::extract(p).unwrap(),
                _ => extract::text::from_markdown(&std::fs::read_to_string(p).unwrap(), ""),
            }
        };
        let drafts = chunk::chunk(&ex.blocks);
        let t = std::time::Instant::now();
        let texts: Vec<String> = drafts
            .iter()
            .map(|d| format!("{}\n{}", ex.title, d.text))
            .collect();
        let vecs = embedder.embed_passages(&texts).unwrap();
        let src = st.insert_source("test", &ex.title, Some(input), None).unwrap();
        let n = drafts.len();
        st.replace_chunks(
            src.id,
            &ex.title,
            drafts
                .into_iter()
                .zip(vecs)
                .map(|(d, v)| store::NewChunk { text: d.text, locator: d.locator, embedding: Some(v) })
                .collect(),
        )
        .unwrap();
        let first = st.chunks_of(src.id).unwrap();
        let loc = first.first().map(|c| serde_json::to_string(&c.locator).unwrap()).unwrap_or_default();
        println!("+ '{}' {} blocks -> {} chunks, embedded in {:?}, first locator {}", ex.title, ex.blocks.len(), n, t.elapsed(), loc);
    }

    for q in queries {
        let t = std::time::Instant::now();
        let hits = search::search(&st, Some(&embedder), q, &Default::default(), 3).unwrap();
        println!("\nQ: {q}   ({:?})", t.elapsed());
        let qv = embedder.embed_query(q).unwrap();
        let top = st.vectors.read().unwrap().top_k(&qv, 4, |_| true);
        let chunks = st.chunks_by_ids(&top.iter().map(|x| x.0).collect::<Vec<_>>()).unwrap();
        let sources: std::collections::HashMap<i64, String> =
            st.list_sources().unwrap().into_iter().map(|s| (s.id, s.title)).collect();
        let sem: Vec<String> = top
            .iter()
            .map(|(id, c)| format!("{:.3} {}", c, sources[&chunks[id].source_id].chars().take(18).collect::<String>()))
            .collect();
        println!("  semantic: {}", sem.join(" | "));
        for h in hits {
            let snippet: String = h.text.chars().take(90).collect::<String>().replace('\n', " ");
            println!(
                "  {:.4} kw={} sem={:?} [{}] {} | {}",
                h.score, h.keyword, h.semantic.map(|s| (s * 1000.0).round() / 1000.0), h.source_title,
                serde_json::to_string(&h.locator).unwrap(), snippet
            );
        }
    }
}
