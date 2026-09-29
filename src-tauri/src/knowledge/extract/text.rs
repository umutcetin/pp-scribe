use super::{Block, Extracted};
use crate::knowledge::store::Locator;

/// Markdown-aware split: `#` headings build a heading path, blank lines separate paragraphs.
pub fn from_markdown(text: &str, title_hint: &str) -> Extracted {
    let mut heads: Vec<(usize, String)> = Vec::new();
    let mut blocks = Vec::new();
    let mut para = String::new();
    let mut first_heading: Option<String> = None;

    let flush = |para: &mut String, heads: &[(usize, String)], blocks: &mut Vec<Block>| {
        let t = para.trim();
        if !t.is_empty() {
            blocks.push(Block {
                text: t.to_string(),
                locator: Locator {
                    heading: heading_path(heads),
                    ..Default::default()
                },
            });
        }
        para.clear();
    };

    let mut in_code = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
        }
        if !in_code {
            let level = trimmed.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
                flush(&mut para, &heads, &mut blocks);
                let h = trimmed[level..].trim().to_string();
                first_heading.get_or_insert_with(|| h.clone());
                heads.retain(|(l, _)| *l < level);
                heads.push((level, h));
                continue;
            }
            if trimmed.is_empty() {
                flush(&mut para, &heads, &mut blocks);
                continue;
            }
        }
        para.push_str(line);
        para.push('\n');
    }
    flush(&mut para, &heads, &mut blocks);

    let title = if !title_hint.trim().is_empty() {
        title_hint.trim().to_string()
    } else {
        first_heading.unwrap_or_else(|| derive_title(text))
    };
    Extracted { title, blocks }
}

pub fn heading_path(heads: &[(usize, String)]) -> Option<String> {
    (!heads.is_empty()).then(|| {
        heads
            .iter()
            .map(|(_, h)| h.as_str())
            .collect::<Vec<_>>()
            .join(" › ")
    })
}

/// First meaningful line, shortened — used for pasted notes without a title.
pub fn derive_title(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("Untitled note");
    let mut t: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        t.push('…');
    }
    t
}
