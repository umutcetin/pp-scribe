//! PDF text via macOS PDFKit, one block per page (locator = page number).
//! Pages without a text layer (scans) are counted; OCR arrives with image support.

use std::path::Path;

use anyhow::{anyhow, Result};

use super::{Block, Extracted};
use crate::knowledge::store::Locator;

pub struct PdfText {
    pub extracted: Extracted,
    pub pages: usize,
    pub pages_without_text: usize,
}

#[cfg(target_os = "macos")]
pub fn extract(path: &Path) -> Result<PdfText> {
    use objc2::AllocAnyThread;
    use objc2_foundation::NSURL;
    use objc2_pdf_kit::PDFDocument;

    let url = NSURL::from_file_path(path).ok_or_else(|| anyhow!("Invalid path"))?;
    let doc = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
        .ok_or_else(|| anyhow!("Could not open PDF (damaged or password protected?)"))?;

    let pages = unsafe { doc.pageCount() };
    let mut blocks = Vec::new();
    let mut empty = 0;
    for i in 0..pages {
        let text = unsafe { doc.pageAtIndex(i) }
            .and_then(|p| unsafe { p.string() })
            .map(|s| s.to_string())
            .unwrap_or_default();
        let text = normalize_page(&text);
        if text.chars().filter(|c| c.is_alphanumeric()).count() < 20 {
            empty += 1;
            continue;
        }
        blocks.push(Block {
            text,
            locator: Locator {
                page: Some(i as u32 + 1),
                ..Default::default()
            },
        });
    }

    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "PDF".into());
    Ok(PdfText {
        extracted: Extracted { title, blocks },
        pages,
        pages_without_text: empty,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn extract(_path: &Path) -> Result<PdfText> {
    Err(anyhow!("PDF import is only supported on macOS"))
}

/// PDFKit breaks every visual line; re-join lines inside paragraphs and fix hyphenation.
fn normalize_page(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.lines().map(str::trim) {
        if line.is_empty() {
            if !out.ends_with("\n\n") {
                out.push_str("\n\n");
            }
            continue;
        }
        if out.is_empty() || out.ends_with('\n') {
            out.push_str(line);
        } else if out.ends_with('-') && line.starts_with(|c: char| c.is_lowercase()) {
            out.pop();
            out.push_str(line);
        } else if out.ends_with(['.', ':', '?', '!']) && line.starts_with(['•', '-', '–', '*']) {
            out.push('\n');
            out.push_str(line);
        } else {
            out.push(' ');
            out.push_str(line);
        }
    }
    out.trim().to_string()
}
