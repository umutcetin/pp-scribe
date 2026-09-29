//! Personal knowledge base: local ingest, embeddings and hybrid search.
//! Nothing here talks to a cloud LLM.

pub mod chunk;
pub mod commands;
pub mod embed;
pub mod extract;
pub mod search;
pub mod store;

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::meeting::{storage as meeting_storage, MeetingManager};
use crate::stt::ModelManager;
use embed::Embedder;
use extract::Extracted;
use store::{NewChunk, Source, Store};

pub const EMBED_MODEL: &str = "bge-m3";

const TEXT_EXTS: &[&str] = &["md", "markdown", "txt", "text", "csv", "json", "log"];

enum Job {
    Index(i64),
    Backfill,
}

#[derive(Serialize, Clone)]
pub struct AddResult {
    pub source: Source,
    pub duplicate: bool,
}

#[derive(Serialize, Clone)]
pub struct KbStatus {
    pub sources: usize,
    pub indexing: usize,
    pub semantic_ready: bool,
    pub missing_embeddings: usize,
    pub root: String,
}

pub struct KnowledgeManager {
    app: AppHandle,
    pub store: Arc<Store>,
    root: PathBuf,
    models: Arc<ModelManager>,
    embedder: Mutex<Option<Arc<Embedder>>>,
    jobs: Mutex<mpsc::Sender<Job>>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

impl KnowledgeManager {
    pub fn new(app: &AppHandle, models: Arc<ModelManager>) -> Result<Arc<Self>> {
        let root = meeting_storage::root(app)?.join("Knowledge");
        std::fs::create_dir_all(root.join("files"))?;
        let store = Arc::new(Store::open(&root.join("kb.sqlite"), EMBED_MODEL)?);

        let (tx, rx) = mpsc::channel::<Job>();
        let kb = Arc::new(Self {
            app: app.clone(),
            store,
            root,
            models,
            embedder: Mutex::new(None),
            jobs: Mutex::new(tx),
        });

        let worker = kb.clone();
        std::thread::spawn(move || worker.run_worker(rx));

        // Resume anything interrupted by a quit, pick up meetings, fill missing vectors.
        for s in kb.store.list_sources()? {
            if s.status == "pending" || s.status == "indexing" {
                kb.enqueue(Job::Index(s.id));
            }
        }
        kb.sync_meetings();
        kb.enqueue(Job::Backfill);
        Ok(kb)
    }

    fn enqueue(&self, job: Job) {
        let _ = self.jobs.lock().unwrap().send(job);
    }

    pub fn files_dir(&self) -> PathBuf {
        self.root.join("files")
    }

    pub fn file_path(&self, s: &Source) -> Option<PathBuf> {
        s.file_rel.as_ref().map(|r| self.root.join(r))
    }

    fn emit_source(&self, id: i64) {
        if let Ok(Some(s)) = self.store.get_source(id) {
            let _ = self.app.emit("kb-source", s);
        }
    }

    // ---------------------------------------------------------------- adding

    pub fn add_file(&self, path: &Path) -> Result<AddResult> {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let kind = match ext.as_str() {
            "pdf" => "pdf",
            "docx" => "docx",
            "md" | "markdown" => "md",
            e if TEXT_EXTS.contains(&e) => "txt",
            "doc" => return Err(anyhow!("Old .doc files aren't supported — save as .docx")),
            _ => return Err(anyhow!("Unsupported file type: .{ext}")),
        };
        let bytes = std::fs::read(path)?;
        let hash = sha256_hex(&bytes);
        if let Some(existing) = self.store.find_by_hash(&hash)? {
            return Ok(AddResult { source: existing, duplicate: true });
        }
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".into());
        let source = self.store.insert_source(kind, &title, Some(&path.to_string_lossy()), Some(&hash))?;
        let rel = format!("files/{}.{}", source.id, ext);
        std::fs::write(self.root.join(&rel), &bytes)?;
        self.store.set_file(source.id, &rel)?;
        self.enqueue(Job::Index(source.id));
        Ok(AddResult { source: self.store.get_source(source.id)?.unwrap(), duplicate: false })
    }

    pub fn add_text(&self, title: &str, text: &str) -> Result<AddResult> {
        let text = text.trim();
        if text.is_empty() {
            return Err(anyhow!("Nothing to add"));
        }
        let hash = sha256_hex(text.as_bytes());
        if let Some(existing) = self.store.find_by_hash(&hash)? {
            return Ok(AddResult { source: existing, duplicate: true });
        }
        let title = if title.trim().is_empty() {
            extract::text::derive_title(text)
        } else {
            title.trim().to_string()
        };
        let source = self.store.insert_source("note", &title, None, Some(&hash))?;
        let rel = format!("files/{}.md", source.id);
        std::fs::write(self.root.join(&rel), text)?;
        self.store.set_file(source.id, &rel)?;
        self.enqueue(Job::Index(source.id));
        Ok(AddResult { source: self.store.get_source(source.id)?.unwrap(), duplicate: false })
    }

    pub fn add_url(&self, url: &str) -> Result<AddResult> {
        let url = url.trim();
        let parsed = reqwest::Url::parse(url).map_err(|_| anyhow!("Not a valid URL"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(anyhow!("Only http(s) links can be added"));
        }
        if let Some(existing) = self.store.find_by_origin("url", parsed.as_str())? {
            return Ok(AddResult { source: existing, duplicate: true });
        }
        let source = self.store.insert_source("url", parsed.as_str(), Some(parsed.as_str()), None)?;
        self.enqueue(Job::Index(source.id));
        Ok(AddResult { source, duplicate: false })
    }

    /// Adds or refreshes a meeting transcript. Meetings the user removed stay excluded.
    pub fn ingest_meeting(&self, meeting_id: &str) {
        let source = match self.store.find_by_origin("meeting", meeting_id) {
            Ok(Some(s)) if s.status == "excluded" => return,
            Ok(Some(s)) => s,
            Ok(None) => {
                let title = meeting_storage::meeting_dir(&self.app, meeting_id)
                    .and_then(|d| meeting_storage::read_meta(&d))
                    .map(|m| m.title)
                    .unwrap_or_else(|_| meeting_id.to_string());
                match self.store.insert_source("meeting", &title, Some(meeting_id), None) {
                    Ok(s) => s,
                    Err(e) => return eprintln!("[kb] meeting source: {e}"),
                }
            }
            Err(e) => return eprintln!("[kb] meeting lookup: {e}"),
        };
        let _ = self.store.set_status(source.id, "pending", None);
        self.emit_source(source.id);
        self.enqueue(Job::Index(source.id));
    }

    pub fn rename_meeting(&self, meeting_id: &str, title: &str) {
        if let Ok(Some(s)) = self.store.find_by_origin("meeting", meeting_id) {
            let _ = self.store.set_title(s.id, title);
            self.emit_source(s.id);
        }
    }

    pub fn forget_meeting(&self, meeting_id: &str) {
        if let Ok(Some(s)) = self.store.find_by_origin("meeting", meeting_id) {
            let _ = self.store.delete_source(s.id);
            let _ = self.app.emit("kb-removed", s.id);
        }
    }

    fn sync_meetings(&self) {
        let Ok(meetings) = meeting_storage::list(&self.app) else { return };
        for m in meetings {
            if m.status == meeting_storage::MeetingStatus::Recording {
                continue;
            }
            if matches!(self.store.find_by_origin("meeting", &m.id), Ok(None)) {
                self.ingest_meeting(&m.id);
            }
        }
    }

    // -------------------------------------------------------------- removing

    pub fn delete_source(&self, id: i64) -> Result<()> {
        let s = self.store.get_source(id)?.ok_or_else(|| anyhow!("Source not found"))?;
        if s.kind == "meeting" {
            // keep a tombstone so startup sync doesn't re-add it
            self.store.replace_chunks(id, &s.title, Vec::new())?;
            self.store.set_status(id, "excluded", None)?;
        } else {
            if let Some(path) = self.file_path(&s) {
                let _ = move_to_trash(&self.app, &path, &s.title);
            }
            self.store.delete_source(id)?;
        }
        let _ = self.app.emit("kb-removed", id);
        Ok(())
    }

    pub fn reindex(&self, id: i64) -> Result<()> {
        self.store.set_status(id, "pending", None)?;
        self.emit_source(id);
        self.enqueue(Job::Index(id));
        Ok(())
    }

    pub fn backfill(&self) {
        self.enqueue(Job::Backfill);
    }

    // ------------------------------------------------------------- searching

    /// Loads e5 on first use if it has been downloaded.
    pub fn embedder(&self) -> Option<Arc<Embedder>> {
        let mut guard = self.embedder.lock().unwrap();
        if guard.is_none() {
            let (dir, _) = self.models.path(EMBED_MODEL).ok()?;
            match Embedder::load(&dir, embed::BGE_M3) {
                Ok(e) => *guard = Some(Arc::new(e)),
                Err(e) => eprintln!("[kb] embedder load failed: {e}"),
            }
        }
        guard.clone()
    }

    pub fn search(&self, query: &str, filter: &search::SearchFilter, limit: usize) -> Result<Vec<search::SearchHit>> {
        let embedder = self.embedder();
        search::search(&self.store, embedder.as_deref(), query, filter, limit)
    }

    pub fn status(&self) -> Result<KbStatus> {
        let sources = self.store.list_sources()?;
        Ok(KbStatus {
            sources: sources.iter().filter(|s| s.status != "excluded").count(),
            indexing: sources.iter().filter(|s| s.status == "pending" || s.status == "indexing").count(),
            semantic_ready: self.models.path(EMBED_MODEL).is_ok(),
            missing_embeddings: self.store.count_missing_embeddings()?,
            root: self.root.to_string_lossy().into_owned(),
        })
    }

    // ---------------------------------------------------------------- worker

    fn recording(&self) -> bool {
        self.app
            .try_state::<Arc<MeetingManager>>()
            .is_some_and(|m| m.active_id().is_some())
    }

    fn run_worker(self: Arc<Self>, rx: mpsc::Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            // Indexing competes with live captions for CPU: wait until the meeting ends.
            while self.recording() {
                std::thread::sleep(Duration::from_secs(3));
            }
            match job {
                Job::Index(id) => {
                    if let Err(e) = self.index(id) {
                        eprintln!("[kb] index {id} failed: {e}");
                        let _ = self.store.set_status(id, "error", Some(&e.to_string()));
                    }
                    self.emit_source(id);
                }
                Job::Backfill => {
                    if let Err(e) = self.run_backfill() {
                        eprintln!("[kb] backfill failed: {e}");
                    }
                }
            }
            let _ = self.app.emit("kb-status", ());
        }
    }

    fn extract(&self, s: &Source) -> Result<Extracted> {
        let file = self.file_path(s);
        match s.kind.as_str() {
            "pdf" => {
                let pdf = extract::pdf::extract(&file.ok_or_else(|| anyhow!("file missing"))?)?;
                if pdf.extracted.blocks.is_empty() {
                    return Err(anyhow!(
                        "No text layer in this PDF ({} pages) — scanned PDFs need OCR, which comes with image support",
                        pdf.pages
                    ));
                }
                Ok(pdf.extracted)
            }
            "docx" => extract::docx::extract(&file.ok_or_else(|| anyhow!("file missing"))?),
            "md" | "txt" | "note" => {
                let text = std::fs::read_to_string(file.ok_or_else(|| anyhow!("file missing"))?)?;
                let hint = if s.kind == "note" { s.title.as_str() } else { "" };
                let mut ex = extract::text::from_markdown(&text, hint);
                if s.kind != "note" {
                    ex.title = s.title.clone();
                }
                Ok(ex)
            }
            "url" => self.extract_url(s),
            "meeting" => extract::meeting::extract(&self.app, s.origin.as_deref().unwrap_or_default()),
            other => Err(anyhow!("Unknown source kind {other}")),
        }
    }

    /// First index fetches the page and keeps a local copy; re-index reads the copy.
    fn extract_url(&self, s: &Source) -> Result<Extracted> {
        if let Some(path) = self.file_path(s) {
            if path.extension().is_some_and(|e| e == "pdf") {
                return Ok(extract::pdf::extract(&path)?.extracted);
            }
            let text = std::fs::read_to_string(&path)?;
            return Ok(extract::text::from_markdown(&text, &s.title));
        }
        let url = s.origin.clone().unwrap_or_default();
        match tauri::async_runtime::block_on(extract::url::fetch(&url))? {
            extract::url::Fetched::Pdf(bytes) => {
                let rel = format!("files/{}.pdf", s.id);
                std::fs::write(self.root.join(&rel), bytes)?;
                self.store.set_file(s.id, &rel)?;
                let mut ex = extract::pdf::extract(&self.root.join(&rel))?.extracted;
                ex.title = url;
                Ok(ex)
            }
            extract::url::Fetched::Page(ex) => {
                let rel = format!("files/{}.md", s.id);
                let body: String = ex.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n\n");
                std::fs::write(self.root.join(&rel), format!("# {}\n\n<{}>\n\n{}", ex.title, url, body))?;
                self.store.set_file(s.id, &rel)?;
                Ok(ex)
            }
        }
    }

    fn index(&self, id: i64) -> Result<()> {
        let Some(s) = self.store.get_source(id)? else { return Ok(()) };
        if s.status == "excluded" {
            return Ok(());
        }
        self.store.set_status(id, "indexing", None)?;
        self.emit_source(id);

        let ex = self.extract(&s)?;
        let drafts = chunk::chunk(&ex.blocks);
        if drafts.is_empty() {
            return Err(anyhow!("No text found"));
        }

        let embeddings = match self.embedder() {
            Some(e) => Some(e.embed_passages(&drafts.iter().map(|d| passage(&ex.title, d.locator.heading.as_deref(), &d.text)).collect::<Vec<_>>())?),
            None => None,
        };
        let chunks = drafts
            .into_iter()
            .enumerate()
            .map(|(i, d)| NewChunk {
                text: d.text,
                locator: d.locator,
                embedding: embeddings.as_ref().map(|e| e[i].clone()),
            })
            .collect();
        self.store.replace_chunks(id, &ex.title, chunks)?;
        Ok(())
    }

    fn run_backfill(&self) -> Result<()> {
        let Some(embedder) = self.embedder() else { return Ok(()) };
        loop {
            let batch = self.store.chunks_missing_embeddings(32)?;
            if batch.is_empty() {
                return Ok(());
            }
            let texts: Vec<String> = batch
                .iter()
                .map(|(c, title)| passage(title, c.locator.heading.as_deref(), &c.text))
                .collect();
            let vecs = embedder.embed_passages(&texts)?;
            for ((c, _), v) in batch.iter().zip(vecs) {
                self.store.set_embedding(c.id, c.source_id, &v)?;
            }
            let _ = self.app.emit("kb-status", ());
        }
    }
}

/// Title and heading give short chunks context the embedding would otherwise miss.
fn passage(title: &str, heading: Option<&str>, text: &str) -> String {
    match heading {
        Some(h) => format!("{title} — {h}\n{text}"),
        None => format!("{title}\n{text}"),
    }
}

fn move_to_trash(app: &AppHandle, path: &Path, title: &str) -> Result<()> {
    let trash = app.path().home_dir()?.join(".Trash");
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let safe: String = title.chars().filter(|c| !matches!(c, '/' | ':')).take(60).collect();
    let mut target = trash.join(format!("PPScribe KB {safe}.{ext}"));
    let mut n = 2;
    while target.exists() {
        target = trash.join(format!("PPScribe KB {safe} {n}.{ext}"));
        n += 1;
    }
    std::fs::rename(path, target)?;
    Ok(())
}
