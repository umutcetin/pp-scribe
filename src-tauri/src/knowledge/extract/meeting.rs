//! Meeting transcript → one block per speaker turn with its time range.

use anyhow::Result;
use tauri::AppHandle;

use super::{Block, Extracted};
use crate::knowledge::store::Locator;
use crate::meeting::storage::{self, MeetingDetail, Speaker};

pub fn extract(app: &AppHandle, meeting_id: &str) -> Result<Extracted> {
    Ok(from_detail(storage::load_detail(app, meeting_id)?))
}

pub fn from_detail(d: MeetingDetail) -> Extracted {
    let segs = d.refined.unwrap_or(d.live);

    let mut blocks: Vec<Block> = Vec::new();
    let mut last: Option<Speaker> = None;
    for s in segs {
        let label = match s.speaker {
            Speaker::Me => &d.meta.me_label,
            Speaker::Them => &d.meta.them_label,
        };
        match blocks.last_mut() {
            // keep turns to ~1 minute so a hit points close to where it was said
            Some(b) if last == Some(s.speaker) && s.start_ms < b.locator.start_ms.unwrap_or(0) + 60_000 => {
                b.text.push(' ');
                b.text.push_str(&s.text);
                b.locator.end_ms = Some(s.end_ms);
            }
            _ => blocks.push(Block {
                text: format!("[{}] {}: {}", storage::format_ts(s.start_ms), label, s.text),
                locator: Locator {
                    start_ms: Some(s.start_ms),
                    end_ms: Some(s.end_ms),
                    ..Default::default()
                },
            }),
        }
        last = Some(s.speaker);
    }

    Extracted {
        title: d.meta.title,
        blocks,
    }
}
