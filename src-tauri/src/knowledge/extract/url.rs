//! Web page → readable article (dom_smoothie) → markdown (htmd) → markdown blocks.

use anyhow::{anyhow, Result};

use super::{text, Extracted};

pub enum Fetched {
    Page(Extracted),
    /// URL pointed at a PDF; caller saves the bytes and imports it as a file.
    Pdf(Vec<u8>),
}

pub async fn fetch(url: &str) -> Result<Fetched> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) PPScribe/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let final_url = resp.url().to_string();

    if ctype.contains("application/pdf") {
        return Ok(Fetched::Pdf(resp.bytes().await?.to_vec()));
    }
    let body = resp.text().await?;
    if ctype.starts_with("text/plain") || ctype.contains("markdown") {
        return Ok(Fetched::Page(text::from_markdown(&body, "")));
    }

    let mut readability = dom_smoothie::Readability::new(body.as_str(), Some(final_url.as_str()), None)
        .map_err(|e| anyhow!("Could not parse page: {e}"))?;
    let article = readability
        .parse()
        .map_err(|e| anyhow!("Could not find readable content: {e}"))?;
    let md = htmd::convert(&article.content).unwrap_or_else(|_| article.text_content.to_string());
    let mut extracted = text::from_markdown(&clean_markdown(&md), &article.title);
    if extracted.title.trim().is_empty() {
        extracted.title = final_url;
    }
    Ok(Fetched::Page(extracted))
}

/// Sections that are link/citation lists rather than content.
const SKIP_SECTIONS: &[&str] = &[
    "references", "notes", "citations", "sources", "external links", "see also", "further reading", "bibliography",
    "kaynakça", "kaynaklar", "notlar", "dış bağlantılar", "ayrıca bakınız",
];

/// Strips what hurts retrieval in web markdown: images, link targets, citation marks and
/// reference sections.
pub fn clean_markdown(md: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    static IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());
    static EMPTY_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\s*\]\([^)]*\)").unwrap());
    static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());
    static CITE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(\d+|[a-z]|↑|\^|citation needed)\]").unwrap());
    // Wikipedia/MediaWiki section edit links, escaped or not
    static EDIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\?\[edit\\?\]").unwrap());

    let mut out = String::new();
    let mut skip_level: Option<usize> = None;
    for raw in md.lines() {
        let line = EDIT.replace_all(raw, "");
        let line = line.as_ref();
        let t = line.trim_start();
        let level = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) {
            let name = t[level..].trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            if skip_level.is_some_and(|l| level <= l) {
                skip_level = None;
            }
            if SKIP_SECTIONS.contains(&name.as_str()) {
                skip_level = Some(level);
            }
        }
        if skip_level.is_some() {
            continue;
        }
        let l = IMAGE.replace_all(line, "");
        let l = EMPTY_LINK.replace_all(&l, "");
        let l = LINK.replace_all(&l, "$1");
        let l = CITE.replace_all(&l, "");
        out.push_str(&l);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleans_links_images_and_reference_sections() {
        let md = "# RAG\n\nSee [LLMs](https://x/llm)[1] and ![fig](a.png)[![](t.png)](b).\n\n## References\\[edit\\]\n\n1. [↑](#c) Foo\n\n## Uses\n\nText";
        let out = super::clean_markdown(md);
        assert!(out.contains("See LLMs and ."), "{out}");
        assert!(!out.contains("Foo"));
        assert!(out.contains("## Uses") && out.contains("Text"));
    }
}
