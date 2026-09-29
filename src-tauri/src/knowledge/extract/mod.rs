//! Turns each source type into ordered text blocks with locators; `chunk.rs` groups them.

pub mod docx;
pub mod meeting;
pub mod pdf;
pub mod text;
pub mod url;

use super::store::Locator;

#[derive(Debug, Clone)]
pub struct Block {
    pub text: String,
    pub locator: Locator,
}

pub struct Extracted {
    pub title: String,
    pub blocks: Vec<Block>,
}
