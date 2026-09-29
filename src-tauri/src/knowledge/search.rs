//! Hybrid retrieval: FTS5 BM25 ∪ e5 cosine, fused with reciprocal rank fusion.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::embed::Embedder;
use super::store::{Locator, Store};

const CANDIDATES: usize = 50;
const RRF_K: f32 = 60.0;
const PER_SOURCE: usize = 3;
/// bge-m3 cosine below which a purely semantic match was noise in tests
/// (relevant cross-lingual hits ≈ 0.46–0.52, unrelated queries ≤ 0.39).
const MIN_SEMANTIC_ONLY: f32 = 0.42;
/// Semantic neighbours below this don't vote in the fusion at all — otherwise every
/// keyword hit also collects a semantic rank just by being among the top 50.
const MIN_SEMANTIC_VOTE: f32 = 0.37;
/// Keyword matches are OR-ed prefix terms, so they are the weaker signal.
const KEYWORD_WEIGHT: f32 = 0.6;
/// A keyword-only hit must rank this high in BM25 to be shown.
const KEYWORD_ONLY_TOP: usize = 10;

#[derive(Deserialize, Default, Debug)]
pub struct SearchFilter {
    pub kinds: Option<Vec<String>>,
    pub source_id: Option<i64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SearchHit {
    pub chunk_id: i64,
    pub source_id: i64,
    pub source_title: String,
    pub source_kind: String,
    pub origin: Option<String>,
    pub text: String,
    pub locator: Locator,
    pub score: f32,
    pub keyword: bool,
    pub semantic: Option<f32>,
}

/// Function words that only add noise to an OR-ed keyword query (Turkish + English).
const STOPWORDS: &[&str] = &[
    "ve", "veya", "ile", "için", "bir", "bu", "şu", "da", "de", "mi", "mı", "mu", "mü", "ne", "nasıl", "neden",
    "hangi", "kim", "var", "yok", "gibi", "daha", "çok", "en", "ki", "ama", "olan", "olarak", "gerekiyor", "nedir",
    "the", "a", "an", "and", "or", "of", "to", "in", "on", "for", "is", "are", "was", "were", "be", "what", "who",
    "how", "why", "when", "which", "do", "does", "did", "it", "this", "that", "with", "as", "at", "by", "from",
];

/// Safe FTS5 expression: every word quoted (no operator injection), prefix-matched, OR-ed.
pub fn fts_expr(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .map(|t| t.to_lowercase())
        .collect();
    let content: Vec<&String> = words.iter().filter(|w| !STOPWORDS.contains(&w.as_str())).collect();
    let chosen = if content.is_empty() { words.iter().collect() } else { content };
    let terms: Vec<String> = chosen
        .into_iter()
        .map(|t| {
            if t.chars().count() >= 3 {
                format!("\"{t}\"*")
            } else {
                format!("\"{t}\"")
            }
        })
        .collect();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

/// Weighted reciprocal rank fusion over (ranked ids, weight) lists.
pub fn rrf(lists: &[(&[i64], f32)]) -> Vec<(i64, f32)> {
    let mut scores: HashMap<i64, f32> = HashMap::new();
    for (list, weight) in lists {
        for (rank, id) in list.iter().enumerate() {
            *scores.entry(*id).or_default() += weight / (RRF_K + rank as f32 + 1.0);
        }
    }
    let mut v: Vec<_> = scores.into_iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

pub fn search(
    store: &Store,
    embedder: Option<&Embedder>,
    query: &str,
    filter: &SearchFilter,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let sources: HashMap<i64, _> = store.list_sources()?.into_iter().map(|s| (s.id, s)).collect();
    let allow = |sid: i64| {
        let Some(s) = sources.get(&sid) else { return false };
        s.status == "ready"
            && filter.source_id.is_none_or(|id| id == sid)
            && filter.kinds.as_ref().is_none_or(|k| k.contains(&s.kind))
    };

    let keyword = match fts_expr(query) {
        Some(expr) => store.keyword_search(&expr, CANDIDATES)?,
        None => Vec::new(),
    };

    let semantic: Vec<(i64, f32)> = match embedder {
        Some(e) => {
            let q = e.embed_query(query)?;
            store.vectors.read().unwrap().top_k(&q, CANDIDATES, allow)
        }
        None => Vec::new(),
    };
    let semantic_ids: Vec<i64> = semantic
        .iter()
        .filter(|(_, cos)| *cos >= MIN_SEMANTIC_VOTE)
        .map(|(id, _)| *id)
        .collect();
    let cosine: HashMap<i64, f32> = semantic.into_iter().collect();

    let fused = rrf(&[(&keyword, KEYWORD_WEIGHT), (&semantic_ids, 1.0)]);
    let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
    let chunks = store.chunks_by_ids(&ids)?;

    let mut per_source: HashMap<i64, usize> = HashMap::new();
    let mut hits = Vec::new();
    for (id, score) in fused {
        let Some(c) = chunks.get(&id) else { continue };
        if !allow(c.source_id) {
            continue;
        }
        let kw_rank = keyword.iter().position(|k| *k == id);
        let sem = cosine.get(&id).copied();
        let semantic_ok = sem.is_some_and(|s| s >= MIN_SEMANTIC_ONLY);
        if !semantic_ok && kw_rank.is_none_or(|r| r >= KEYWORD_ONLY_TOP) {
            continue;
        }
        let is_kw = kw_rank.is_some();
        let n = per_source.entry(c.source_id).or_default();
        if *n >= PER_SOURCE {
            continue;
        }
        *n += 1;
        let s = &sources[&c.source_id];
        hits.push(SearchHit {
            chunk_id: id,
            source_id: c.source_id,
            source_title: s.title.clone(),
            source_kind: s.kind.clone(),
            origin: s.origin.clone(),
            text: c.text.clone(),
            locator: c.locator.clone(),
            score,
            keyword: is_kw,
            semantic: sem,
        });
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_expr_quotes_and_prefixes() {
        assert_eq!(fts_expr("Bütçe AND x-ray \"NEAR\""), Some("\"bütçe\"* OR \"ray\"* OR \"near\"*".into()));
        assert_eq!(fts_expr("için ne var"), Some("\"için\"* OR \"ne\" OR \"var\"*".into()), "all stopwords: keep them");
        assert_eq!(fts_expr("?? !"), None);
    }

    #[test]
    fn rrf_rewards_agreement() {
        let a = [1, 2, 3];
        let b = [3, 4];
        let fused = rrf(&[(&a, 1.0), (&b, 1.0)]);
        assert_eq!(fused[0].0, 3, "in both lists beats first place in one");
    }
}
