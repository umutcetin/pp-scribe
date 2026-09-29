//! SQLite store for the knowledge base: sources, chunks, FTS5 keyword index and
//! embeddings (f32 BLOBs, mirrored in memory for brute-force cosine search).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, RwLock};

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Locator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_end: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Source {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub origin: Option<String>,
    pub file_rel: Option<String>,
    pub added_at: String,
    pub status: String,
    pub error: Option<String>,
    pub chunk_count: i64,
    pub char_count: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Chunk {
    pub id: i64,
    pub source_id: i64,
    pub ord: i64,
    pub text: String,
    pub locator: Locator,
}

pub struct NewChunk {
    pub text: String,
    pub locator: Locator,
    pub embedding: Option<Vec<f32>>,
}

/// In-memory copy of every stored embedding.
#[derive(Default)]
pub struct VecIndex {
    pub dim: usize,
    pub chunk_ids: Vec<i64>,
    pub source_ids: Vec<i64>,
    pub data: Vec<f32>,
}

impl VecIndex {
    fn push(&mut self, chunk_id: i64, source_id: i64, v: &[f32]) {
        if self.dim == 0 {
            self.dim = v.len();
        }
        if v.len() != self.dim {
            return;
        }
        self.chunk_ids.push(chunk_id);
        self.source_ids.push(source_id);
        self.data.extend_from_slice(v);
    }

    fn remove_source(&mut self, source_id: i64) {
        let dim = self.dim;
        let mut keep = VecIndex { dim, ..Default::default() };
        for i in 0..self.chunk_ids.len() {
            if self.source_ids[i] != source_id {
                keep.push(self.chunk_ids[i], self.source_ids[i], &self.data[i * dim..(i + 1) * dim]);
            }
        }
        *self = keep;
    }

    /// Top-k by dot product (vectors are L2-normalized, so this is cosine similarity).
    pub fn top_k(&self, q: &[f32], k: usize, allow: impl Fn(i64) -> bool) -> Vec<(i64, f32)> {
        if self.dim == 0 || q.len() != self.dim {
            return Vec::new();
        }
        let mut scored: Vec<(i64, f32)> = (0..self.chunk_ids.len())
            .filter(|&i| allow(self.source_ids[i]))
            .map(|i| {
                let row = &self.data[i * self.dim..(i + 1) * self.dim];
                (self.chunk_ids[i], row.iter().zip(q).map(|(a, b)| a * b).sum())
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored
    }
}

pub struct Store {
    conn: Mutex<Connection>,
    pub vectors: RwLock<VecIndex>,
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

const SOURCE_COLS: &str =
    "id, kind, title, origin, file_rel, added_at, status, error, chunk_count, char_count";

fn row_to_source(r: &rusqlite::Row) -> rusqlite::Result<Source> {
    Ok(Source {
        id: r.get(0)?,
        kind: r.get(1)?,
        title: r.get(2)?,
        origin: r.get(3)?,
        file_rel: r.get(4)?,
        added_at: r.get(5)?,
        status: r.get(6)?,
        error: r.get(7)?,
        chunk_count: r.get(8)?,
        char_count: r.get(9)?,
    })
}

impl Store {
    /// Opens (and migrates) the store. Embeddings made by a different model than
    /// `embed_model` are dropped so the backfill recomputes them.
    pub fn open(path: &Path, embed_model: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS sources (
                 id INTEGER PRIMARY KEY,
                 kind TEXT NOT NULL,
                 title TEXT NOT NULL,
                 origin TEXT,
                 file_rel TEXT,
                 added_at TEXT NOT NULL,
                 content_hash TEXT,
                 status TEXT NOT NULL,
                 error TEXT,
                 chunk_count INTEGER NOT NULL DEFAULT 0,
                 char_count INTEGER NOT NULL DEFAULT 0,
                 summary_local TEXT,
                 summary_claude TEXT
             );
             CREATE INDEX IF NOT EXISTS sources_hash ON sources(content_hash);
             CREATE INDEX IF NOT EXISTS sources_origin ON sources(kind, origin);
             CREATE TABLE IF NOT EXISTS chunks (
                 id INTEGER PRIMARY KEY,
                 source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                 ord INTEGER NOT NULL,
                 text TEXT NOT NULL,
                 locator TEXT NOT NULL,
                 embedding BLOB
             );
             CREATE INDEX IF NOT EXISTS chunks_source ON chunks(source_id, ord);
             CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                 text, title, tokenize = 'unicode61 remove_diacritics 2'
             );
             CREATE TABLE IF NOT EXISTS kb_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        let current: Option<String> = conn
            .query_row("SELECT value FROM kb_meta WHERE key = 'embed_model'", [], |r| r.get(0))
            .optional()?;
        if current.as_deref() != Some(embed_model) {
            conn.execute("UPDATE chunks SET embedding = NULL", [])?;
            conn.execute(
                "INSERT OR REPLACE INTO kb_meta (key, value) VALUES ('embed_model', ?1)",
                [embed_model],
            )?;
        }

        let mut index = VecIndex::default();
        {
            let mut stmt = conn.prepare(
                "SELECT id, source_id, embedding FROM chunks WHERE embedding IS NOT NULL ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Vec<u8>>(2)?))
            })?;
            for row in rows {
                let (id, sid, blob) = row?;
                index.push(id, sid, &from_blob(&blob));
            }
        }

        Ok(Self {
            conn: Mutex::new(conn),
            vectors: RwLock::new(index),
        })
    }

    pub fn insert_source(
        &self,
        kind: &str,
        title: &str,
        origin: Option<&str>,
        content_hash: Option<&str>,
    ) -> Result<Source> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sources (kind, title, origin, added_at, content_hash, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
            params![kind, title, origin, chrono::Local::now().to_rfc3339(), content_hash],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_source(id)?.ok_or_else(|| anyhow::anyhow!("insert failed"))
    }

    pub fn get_source(&self, id: i64) -> Result<Option<Source>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                &format!("SELECT {SOURCE_COLS} FROM sources WHERE id = ?1"),
                [id],
                row_to_source,
            )
            .optional()?)
    }

    pub fn find_by_hash(&self, hash: &str) -> Result<Option<Source>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                &format!("SELECT {SOURCE_COLS} FROM sources WHERE content_hash = ?1"),
                [hash],
                row_to_source,
            )
            .optional()?)
    }

    pub fn find_by_origin(&self, kind: &str, origin: &str) -> Result<Option<Source>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                &format!("SELECT {SOURCE_COLS} FROM sources WHERE kind = ?1 AND origin = ?2"),
                [kind, origin],
                row_to_source,
            )
            .optional()?)
    }

    pub fn list_sources(&self) -> Result<Vec<Source>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SOURCE_COLS} FROM sources ORDER BY added_at DESC"
        ))?;
        let rows = stmt.query_map([], row_to_source)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn source_kinds(&self) -> Result<HashMap<i64, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, kind FROM sources")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_status(&self, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sources SET status = ?2, error = ?3 WHERE id = ?1",
            params![id, status, error],
        )?;
        Ok(())
    }

    pub fn set_file(&self, id: i64, file_rel: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sources SET file_rel = ?2 WHERE id = ?1",
            params![id, file_rel],
        )?;
        Ok(())
    }

    pub fn set_title(&self, id: i64, title: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE sources SET title = ?2 WHERE id = ?1", params![id, title])?;
        conn.execute(
            "UPDATE chunks_fts SET title = ?2 WHERE rowid IN (SELECT id FROM chunks WHERE source_id = ?1)",
            params![id, title],
        )?;
        Ok(())
    }

    /// Replaces all chunks of a source atomically and marks it ready.
    pub fn replace_chunks(&self, source_id: i64, title: &str, chunks: Vec<NewChunk>) -> Result<()> {
        let char_count: usize = chunks.iter().map(|c| c.text.chars().count()).sum();
        let mut inserted = Vec::with_capacity(chunks.len());
        {
            let mut conn = self.conn.lock().unwrap();
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM chunks WHERE source_id = ?1)",
                [source_id],
            )?;
            tx.execute("DELETE FROM chunks WHERE source_id = ?1", [source_id])?;
            for (ord, c) in chunks.iter().enumerate() {
                tx.execute(
                    "INSERT INTO chunks (source_id, ord, text, locator, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        source_id,
                        ord as i64,
                        c.text,
                        serde_json::to_string(&c.locator)?,
                        c.embedding.as_deref().map(to_blob)
                    ],
                )?;
                let id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO chunks_fts (rowid, text, title) VALUES (?1, ?2, ?3)",
                    params![id, c.text, title],
                )?;
                inserted.push(id);
            }
            tx.execute(
                "UPDATE sources SET status = 'ready', error = NULL, chunk_count = ?2, char_count = ?3, title = ?4 WHERE id = ?1",
                params![source_id, chunks.len() as i64, char_count as i64, title],
            )?;
            tx.commit()?;
        }

        let mut idx = self.vectors.write().unwrap();
        idx.remove_source(source_id);
        for (id, c) in inserted.iter().zip(&chunks) {
            if let Some(v) = &c.embedding {
                idx.push(*id, source_id, v);
            }
        }
        Ok(())
    }

    pub fn delete_source(&self, id: i64) -> Result<()> {
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM chunks WHERE source_id = ?1)",
                [id],
            )?;
            conn.execute("DELETE FROM chunks WHERE source_id = ?1", [id])?;
            conn.execute("DELETE FROM sources WHERE id = ?1", [id])?;
        }
        self.vectors.write().unwrap().remove_source(id);
        Ok(())
    }

    pub fn chunks_of(&self, source_id: i64) -> Result<Vec<Chunk>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, ord, text, locator FROM chunks WHERE source_id = ?1 ORDER BY ord",
        )?;
        let rows = stmt.query_map([source_id], row_to_chunk)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn chunks_by_ids(&self, ids: &[i64]) -> Result<HashMap<i64, Chunk>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, ord, text, locator FROM chunks WHERE id = ?1",
        )?;
        let mut out = HashMap::new();
        for id in ids {
            if let Some(c) = stmt.query_row([id], row_to_chunk).optional()? {
                out.insert(*id, c);
            }
        }
        Ok(out)
    }

    /// Chunks still waiting for an embedding (e.g. added before the model was downloaded).
    pub fn chunks_missing_embeddings(&self, limit: usize) -> Result<Vec<(Chunk, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.source_id, c.ord, c.text, c.locator, s.title
             FROM chunks c JOIN sources s ON s.id = c.source_id
             WHERE c.embedding IS NULL ORDER BY c.id LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| Ok((row_to_chunk(r)?, r.get::<_, String>(5)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn count_missing_embeddings(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM chunks WHERE embedding IS NULL", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn set_embedding(&self, chunk_id: i64, source_id: i64, v: &[f32]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE chunks SET embedding = ?2 WHERE id = ?1",
            params![chunk_id, to_blob(v)],
        )?;
        self.vectors.write().unwrap().push(chunk_id, source_id, v);
        Ok(())
    }

    /// BM25 keyword search. `match_expr` must already be a safe FTS5 expression.
    pub fn keyword_search(&self, match_expr: &str, k: usize) -> Result<Vec<i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY bm25(chunks_fts, 1.0, 0.5) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![match_expr, k as i64], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn row_to_chunk(r: &rusqlite::Row) -> rusqlite::Result<Chunk> {
    let locator: String = r.get(4)?;
    Ok(Chunk {
        id: r.get(0)?,
        source_id: r.get(1)?,
        ord: r.get(2)?,
        text: r.get(3)?,
        locator: serde_json::from_str(&locator).unwrap_or_default(),
    })
}
