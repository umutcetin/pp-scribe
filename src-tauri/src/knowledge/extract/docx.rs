//! .docx = zip with word/document.xml. Paragraph text + heading detection → heading path.
//!
//! Headings are found three ways, in order: the paragraph style's built-in name or outline
//! level from styles.xml (style *ids* are localized, e.g. "Balk1" in Turkish Word, names are
//! not), an outline level set on the paragraph itself, and — for documents formatted by hand —
//! short, fully bold, larger-than-body paragraphs.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::text::heading_path;
use super::{Block, Extracted};
use crate::knowledge::store::Locator;

enum StyleKind {
    Title,
    Heading(usize),
}

fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .map(|a| a.value.into_owned())
}

fn read_part(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut s = String::new();
    zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
    Some(s)
}

/// styleId → Title / Heading(level), from built-in names ("heading 2") or w:outlineLvl.
fn style_map(xml: &str) -> HashMap<String, StyleKind> {
    let mut map = HashMap::new();
    let mut reader = Reader::from_str(xml);
    let mut id: Option<String> = None;
    let mut kind: Option<StyleKind> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == "style" => {
                id = attr(&e, "styleId");
                kind = None;
            }
            Ok(Event::Start(e) | Event::Empty(e)) => match e.local_name().as_ref() {
                "name" => {
                    let name = attr(&e, "val").unwrap_or_default().to_lowercase();
                    if name == "title" {
                        kind = Some(StyleKind::Title);
                    } else if let Some(n) = name.strip_prefix("heading ").and_then(|n| n.parse().ok()) {
                        kind = Some(StyleKind::Heading(n));
                    }
                }
                "outlineLvl" if kind.is_none() => {
                    if let Some(n) = attr(&e, "val").and_then(|v| v.parse::<usize>().ok()) {
                        if n < 9 {
                            kind = Some(StyleKind::Heading(n + 1));
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if e.local_name().as_ref() == "style" => {
                if let (Some(i), Some(k)) = (id.take(), kind.take()) {
                    map.insert(i, k);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    map
}

fn core_title(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut in_title = false;
    let mut title = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == "title" => in_title = true,
            Ok(Event::End(e)) if e.local_name().as_ref() == "title" => break,
            Ok(Event::Text(t)) if in_title => title.push_str(&t.xml10_content()),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let t = title.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[derive(Default)]
struct Para {
    text: String,
    style: Option<String>,
    outline: Option<usize>,
    /// Every run that carried text was bold.
    all_bold: bool,
    max_size: u32,
    // current run state
    run_bold: bool,
    run_size: u32,
    run_has_text: bool,
}

pub fn extract(path: &Path) -> Result<Extracted> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let xml = read_part(&mut zip, "word/document.xml")
        .ok_or_else(|| anyhow!("Not a Word document (word/document.xml missing)"))?;
    let styles = read_part(&mut zip, "word/styles.xml")
        .map(|s| style_map(&s))
        .unwrap_or_default();
    let meta_title = read_part(&mut zip, "docProps/core.xml").and_then(|s| core_title(&s));

    // Pass 1: paragraphs with their formatting.
    let mut paras: Vec<Para> = Vec::new();
    let mut reader = Reader::from_str(&xml);
    let mut p: Option<Para> = None;
    let mut in_rpr = false;
    let mut in_text = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == "p" => {
                p = Some(Para { all_bold: true, ..Default::default() });
            }
            Event::End(e) if e.local_name().as_ref() == "p" => {
                if let Some(mut done) = p.take() {
                    done.text = done.text.trim().to_string();
                    if !done.text.is_empty() {
                        paras.push(done);
                    }
                }
            }
            Event::Start(e) | Event::Empty(e) => {
                let Some(para) = p.as_mut() else { continue };
                match e.local_name().as_ref() {
                    "pStyle" => para.style = attr(&e, "val"),
                    "outlineLvl" => para.outline = attr(&e, "val").and_then(|v| v.parse().ok()),
                    "r" => {
                        para.run_bold = false;
                        para.run_size = 0;
                        para.run_has_text = false;
                    }
                    "rPr" => in_rpr = true,
                    "b" if in_rpr => {
                        para.run_bold = !matches!(attr(&e, "val").as_deref(), Some("0" | "false"));
                    }
                    "sz" if in_rpr => para.run_size = attr(&e, "val").and_then(|v| v.parse().ok()).unwrap_or(0),
                    "t" => in_text = true,
                    "tab" => para.text.push('\t'),
                    "br" | "cr" => para.text.push('\n'),
                    _ => {}
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "rPr" => in_rpr = false,
                "t" => in_text = false,
                "r" => {
                    if let Some(para) = p.as_mut() {
                        if para.run_has_text {
                            para.all_bold &= para.run_bold;
                            para.max_size = para.max_size.max(para.run_size);
                        }
                    }
                }
                _ => {}
            },
            Event::Text(t) if in_text => {
                if let Some(para) = p.as_mut() {
                    let s = t.xml10_content();
                    para.run_has_text |= !s.trim().is_empty();
                    para.text.push_str(&s);
                }
            }
            // entities like &amp; / &#233; arrive as separate events since quick-xml 0.38
            Event::GeneralRef(r) if in_text => {
                if let Some(para) = p.as_mut() {
                    if let Ok(Some(c)) = r.resolve_char_ref() {
                        para.text.push(c);
                    } else {
                        para.text.push_str(match r.as_ref() {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "quot" => "\"",
                            "apos" => "'",
                            _ => "",
                        });
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    // Body text size = most common run size, for the "looks like a heading" fallback.
    let mut sizes: HashMap<u32, usize> = HashMap::new();
    for para in &paras {
        *sizes.entry(para.max_size).or_default() += para.text.len();
    }
    let body_size = sizes.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s).unwrap_or(0);

    // Pass 2: classify.
    let mut heads: Vec<(usize, String)> = Vec::new();
    let mut blocks = Vec::new();
    let mut title = meta_title;
    let mut first_h1: Option<String> = None;
    for para in paras {
        let styled = para.style.as_ref().and_then(|s| styles.get(s));
        let level = match styled {
            Some(StyleKind::Title) => {
                title.get_or_insert(para.text.clone());
                continue;
            }
            Some(StyleKind::Heading(n)) => Some(*n),
            None => para.outline.filter(|n| *n < 9).map(|n| n + 1).or_else(|| {
                let short = para.text.chars().count() <= 100 && !para.text.ends_with(['.', ':', ';', ',']);
                (short && para.all_bold && para.max_size > body_size).then(|| {
                    if para.max_size >= body_size * 2 {
                        1
                    } else if para.max_size * 2 >= body_size * 3 {
                        2
                    } else {
                        3
                    }
                })
            }),
        };

        match level {
            Some(level) => {
                if level == 1 {
                    first_h1.get_or_insert(para.text.clone());
                }
                heads.retain(|(l, _)| *l < level);
                heads.push((level, para.text));
            }
            None => blocks.push(Block {
                text: para.text,
                locator: Locator {
                    heading: heading_path(&heads),
                    ..Default::default()
                },
            }),
        }
    }

    let title = title.or(first_h1).unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".into())
    });
    Ok(Extracted { title, blocks })
}
