//! On-disk layout — one folder per meeting under ~/Documents/PPScribe:
//!
//!   meta.json      title, times, status, models used
//!   me.wav         my microphone, 16 kHz mono
//!   them.wav       remote participants (system audio), 16 kHz mono
//!   live.jsonl     live captions, appended as they are finalized (crash-safe)
//!   refined.json   accurate transcript from the recordings
//!   transcript.md  human-readable export

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    Me,
    Them,
}

impl Speaker {
    pub fn key(&self) -> &'static str {
        match self {
            Speaker::Me => "me",
            Speaker::Them => "them",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Segment {
    pub id: String,
    pub speaker: Speaker,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Recording,
    Recorded,
    Refining,
    Refined,
    RefineFailed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MeetingMeta {
    pub id: String,
    pub title: String,
    pub started_at: DateTime<Local>,
    pub ended_at: Option<DateTime<Local>>,
    pub duration_ms: u64,
    pub status: MeetingStatus,
    pub language: String,
    pub live_model: String,
    pub refine_model: Option<String>,
    pub refine_error: Option<String>,
    pub me_label: String,
    pub them_label: String,
    pub me_device: Option<String>,
    pub them_device: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct MeetingDetail {
    pub meta: MeetingMeta,
    pub dir: String,
    pub live: Vec<Segment>,
    pub refined: Option<Vec<Segment>>,
}

pub fn root(app: &AppHandle) -> Result<PathBuf> {
    let dir = app.path().document_dir()?.join("PPScribe");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn meeting_dir(app: &AppHandle, id: &str) -> Result<PathBuf> {
    // ids are folder names we generated; refuse anything that could escape the root
    if id.is_empty() || id.contains('/') || id.contains("..") {
        return Err(anyhow!("Invalid meeting id"));
    }
    let dir = root(app)?.join(id);
    if !dir.is_dir() {
        return Err(anyhow!("Meeting not found: {}", id));
    }
    Ok(dir)
}

pub fn create_meeting_dir(app: &AppHandle, started_at: &DateTime<Local>) -> Result<(String, PathBuf)> {
    let root = root(app)?;
    let base = started_at.format("%Y-%m-%d_%H%M%S").to_string();
    let mut id = base.clone();
    let mut n = 2;
    while root.join(&id).exists() {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    let dir = root.join(&id);
    fs::create_dir_all(&dir)?;
    Ok((id, dir))
}

pub fn wav_path(dir: &Path, speaker: Speaker) -> PathBuf {
    dir.join(format!("{}.wav", speaker.key()))
}

pub fn write_meta(dir: &Path, meta: &MeetingMeta) -> Result<()> {
    let tmp = dir.join("meta.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(meta)?)?;
    fs::rename(tmp, dir.join("meta.json"))?;
    Ok(())
}

pub fn read_meta(dir: &Path) -> Result<MeetingMeta> {
    Ok(serde_json::from_slice(&fs::read(dir.join("meta.json"))?)?)
}

pub fn append_live(dir: &Path, seg: &Segment) -> Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("live.jsonl"))?;
    writeln!(f, "{}", serde_json::to_string(seg)?)?;
    Ok(())
}

pub fn read_live(dir: &Path) -> Vec<Segment> {
    let Ok(f) = fs::File::open(dir.join("live.jsonl")) else {
        return Vec::new();
    };
    let mut segs: Vec<Segment> = BufReader::new(f)
        .lines()
        .map_while(|l| l.ok())
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect();
    segs.sort_by_key(|s| s.start_ms);
    segs
}

pub fn write_refined(dir: &Path, segs: &[Segment]) -> Result<()> {
    fs::write(dir.join("refined.json"), serde_json::to_vec_pretty(segs)?)?;
    Ok(())
}

pub fn read_refined(dir: &Path) -> Option<Vec<Segment>> {
    serde_json::from_slice(&fs::read(dir.join("refined.json")).ok()?).ok()
}

pub fn load_detail(app: &AppHandle, id: &str) -> Result<MeetingDetail> {
    let dir = meeting_dir(app, id)?;
    Ok(MeetingDetail {
        meta: read_meta(&dir)?,
        live: read_live(&dir),
        refined: read_refined(&dir),
        dir: dir.to_string_lossy().into_owned(),
    })
}

/// All meetings, newest first. Meetings left in `Recording` by a crash are repaired.
pub fn list(app: &AppHandle, active_id: Option<&str>) -> Result<Vec<MeetingMeta>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root(app)?)? {
        let dir = entry?.path();
        let Ok(mut meta) = read_meta(&dir) else { continue };
        let interrupted = matches!(meta.status, MeetingStatus::Recording | MeetingStatus::Refining)
            && Some(meta.id.as_str()) != active_id;
        if interrupted && meta.status == MeetingStatus::Recording {
            meta.status = MeetingStatus::Recorded;
            if let Some(last) = read_live(&dir).last() {
                meta.duration_ms = meta.duration_ms.max(last.end_ms);
            }
            let _ = write_meta(&dir, &meta);
        }
        out.push(meta);
    }
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(out)
}

/// Moves the meeting folder to the macOS Trash so a mistaken delete is recoverable.
pub fn trash(app: &AppHandle, id: &str) -> Result<()> {
    let dir = meeting_dir(app, id)?;
    let trash = app.path().home_dir()?.join(".Trash");
    let mut target = trash.join(format!("PPScribe {}", id));
    let mut n = 2;
    while target.exists() {
        target = trash.join(format!("PPScribe {} {}", id, n));
        n += 1;
    }
    fs::rename(&dir, &target)?;
    Ok(())
}

pub fn format_ts(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// Writes transcript.md (refined transcript when available, else live captions).
pub fn export_markdown(app: &AppHandle, id: &str) -> Result<PathBuf> {
    let d = load_detail(app, id)?;
    let (segs, source) = match &d.refined {
        Some(r) => (r.clone(), "refined from recording"),
        None => (d.live.clone(), "live captions"),
    };

    let mut md = format!("# {}\n\n", d.meta.title);
    md.push_str(&format!(
        "- Date: {}\n- Duration: {}\n- Transcript: {}\n\n---\n\n",
        d.meta.started_at.format("%Y-%m-%d %H:%M"),
        format_ts(d.meta.duration_ms),
        source
    ));

    // Consecutive lines from the same speaker become one paragraph.
    let mut last: Option<Speaker> = None;
    for s in &segs {
        let label = match s.speaker {
            Speaker::Me => &d.meta.me_label,
            Speaker::Them => &d.meta.them_label,
        };
        if last == Some(s.speaker) {
            md.push(' ');
            md.push_str(&s.text);
        } else {
            if last.is_some() {
                md.push_str("\n\n");
            }
            md.push_str(&format!("**{}** [{}]: {}", label, format_ts(s.start_ms), s.text));
        }
        last = Some(s.speaker);
    }
    md.push('\n');

    let path = PathBuf::from(&d.dir).join("transcript.md");
    fs::write(&path, md)?;
    Ok(path)
}
