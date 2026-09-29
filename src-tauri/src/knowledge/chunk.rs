//! Groups extracted blocks into retrieval chunks of ~TARGET chars (≈300–400 e5 tokens),
//! never mixing headings, carrying a small overlap, and merging locators (page / time range).

use super::extract::Block;
use super::store::Locator;

const TARGET: usize = 1200;
const MAX: usize = 1800;
const OVERLAP_MAX: usize = 300;

#[derive(Debug, Clone)]
pub struct ChunkDraft {
    pub text: String,
    pub locator: Locator,
}

pub fn chunk(blocks: &[Block]) -> Vec<ChunkDraft> {
    let mut out: Vec<ChunkDraft> = Vec::new();
    let mut cur: Vec<Block> = Vec::new();
    let mut cur_len = 0;

    let emit = |cur: &[Block], out: &mut Vec<ChunkDraft>| {
        if cur.is_empty() {
            return;
        }
        let text = cur.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n\n");
        out.push(ChunkDraft {
            text,
            locator: merge_locators(cur),
        });
    };

    for block in blocks.iter().flat_map(split_long) {
        let len = block.text.chars().count();
        let heading_changed = cur
            .last()
            .is_some_and(|b| b.locator.heading != block.locator.heading);

        if !cur.is_empty() && (heading_changed || cur_len + len > TARGET) {
            emit(&cur, &mut out);
            // overlap: carry the last short block into the next chunk (same heading only)
            let carry = cur
                .last()
                .filter(|b| !heading_changed && b.text.chars().count() <= OVERLAP_MAX && cur.len() > 1)
                .cloned();
            cur.clear();
            cur_len = 0;
            if let Some(b) = carry {
                cur_len += b.text.chars().count();
                cur.push(b);
            }
        }
        cur_len += len;
        cur.push(block);
    }
    emit(&cur, &mut out);
    out
}

/// Blocks longer than MAX are cut at sentence boundaries (falling back to whitespace).
fn split_long(block: &Block) -> Vec<Block> {
    let chars: Vec<char> = block.text.chars().collect();
    if chars.len() <= MAX {
        return vec![block.clone()];
    }
    let mut parts = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let hard_end = (start + TARGET).min(chars.len());
        let mut end = hard_end;
        if hard_end < chars.len() {
            let window = &chars[start + TARGET / 2..hard_end];
            if let Some(p) = window.iter().rposition(|c| matches!(c, '.' | '?' | '!' | '\n')) {
                end = start + TARGET / 2 + p + 1;
            } else if let Some(p) = window.iter().rposition(|c| c.is_whitespace()) {
                end = start + TARGET / 2 + p + 1;
            }
        }
        let text: String = chars[start..end].iter().collect::<String>().trim().to_string();
        if !text.is_empty() {
            parts.push(Block {
                text,
                locator: block.locator.clone(),
            });
        }
        start = end;
    }
    parts
}

fn merge_locators(blocks: &[Block]) -> Locator {
    let first = &blocks[0].locator;
    let last = &blocks[blocks.len() - 1].locator;
    Locator {
        page: first.page,
        page_end: last.page.filter(|p| Some(*p) != first.page),
        heading: first.heading.clone(),
        start_ms: first.start_ms,
        end_ms: last.end_ms.or(last.start_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(text: &str, heading: Option<&str>, page: Option<u32>) -> Block {
        Block {
            text: text.into(),
            locator: Locator {
                heading: heading.map(Into::into),
                page,
                ..Default::default()
            },
        }
    }

    #[test]
    fn groups_small_blocks_and_splits_on_heading() {
        let blocks = vec![b("a", Some("H1"), None), b("b", Some("H1"), None), b("c", Some("H2"), None)];
        let c = chunk(&blocks);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].text, "a\n\nb");
        assert_eq!(c[1].locator.heading.as_deref(), Some("H2"));
    }

    #[test]
    fn page_range_and_overlap() {
        let long = "x".repeat(700);
        let blocks = vec![b(&long, None, Some(1)), b("short tail", None, Some(2)), b(&long, None, Some(3))];
        let c = chunk(&blocks);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].locator.page, c[0].locator.page_end), (Some(1), Some(2)));
        assert!(c[1].text.starts_with("short tail"), "overlap carries the short block");
    }

    #[test]
    fn long_block_is_split_under_max() {
        let text = "Sentence number one is here. ".repeat(200);
        let c = chunk(&[b(&text, None, Some(4))]);
        assert!(c.len() > 3);
        assert!(c.iter().all(|x| x.text.chars().count() <= MAX));
        assert!(c.iter().all(|x| x.locator.page == Some(4)));
    }
}
