use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use super::search::{SearchFilter, SearchHit};
use super::store::{Chunk, Source};
use super::{AddResult, KbStatus, KnowledgeManager};

type Kb<'a> = State<'a, Arc<KnowledgeManager>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn kb_status(kb: Kb) -> Result<KbStatus, String> {
    kb.status().map_err(err)
}

#[tauri::command]
pub fn kb_list_sources(kb: Kb) -> Result<Vec<Source>, String> {
    Ok(kb
        .store
        .list_sources()
        .map_err(err)?
        .into_iter()
        .filter(|s| s.status != "excluded")
        .collect())
}

#[derive(Serialize)]
pub struct SourceDetail {
    pub source: Source,
    pub chunks: Vec<Chunk>,
    pub file_path: Option<String>,
}

#[tauri::command]
pub fn kb_get_source(kb: Kb, id: i64) -> Result<SourceDetail, String> {
    let source = kb.store.get_source(id).map_err(err)?.ok_or("Source not found")?;
    Ok(SourceDetail {
        file_path: kb.file_path(&source).map(|p| p.to_string_lossy().into_owned()),
        chunks: kb.store.chunks_of(id).map_err(err)?,
        source,
    })
}

#[tauri::command]
pub fn kb_add_files(kb: Kb, paths: Vec<String>) -> Vec<Result<AddResult, String>> {
    paths
        .iter()
        .map(|p| kb.add_file(&PathBuf::from(p)).map_err(|e| format!("{}: {}", file_name(p), e)))
        .collect()
}

fn file_name(p: &str) -> String {
    PathBuf::from(p)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

#[tauri::command]
pub fn kb_add_text(kb: Kb, title: String, text: String) -> Result<AddResult, String> {
    kb.add_text(&title, &text).map_err(err)
}

#[tauri::command]
pub fn kb_add_url(kb: Kb, url: String) -> Result<AddResult, String> {
    kb.add_url(&url).map_err(err)
}

#[tauri::command]
pub fn kb_delete_source(kb: Kb, id: i64) -> Result<(), String> {
    kb.delete_source(id).map_err(err)
}

#[tauri::command]
pub fn kb_reindex_source(kb: Kb, id: i64) -> Result<(), String> {
    kb.reindex(id).map_err(err)
}

#[tauri::command]
pub fn kb_backfill(kb: Kb) {
    kb.backfill();
}

#[tauri::command]
pub async fn kb_search(
    kb: State<'_, Arc<KnowledgeManager>>,
    query: String,
    filter: Option<SearchFilter>,
    limit: Option<usize>,
) -> Result<Vec<SearchHit>, String> {
    let kb = kb.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        kb.search(&query, &filter.unwrap_or_default(), limit.unwrap_or(12))
    })
    .await
    .map_err(err)?
    .map_err(err)
}
