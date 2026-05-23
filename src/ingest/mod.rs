//! Document ingest pipeline: parsing, queueing, and dispatching to pagebridge.

pub mod docx;
pub mod epub;
pub mod html;
pub mod pdf;
pub mod queue;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, Result};
use pagebridge::SourceKind;

/// A document after format-specific parsing, ready for pagebridge.
pub struct ParsedDocument {
    pub title: String,
    pub text: String,
    pub source_kind: SourceKind,
    pub metadata: BTreeMap<String, String>,
}

/// Dispatch by file extension or content-type.
pub fn parse_bytes(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    let ext = Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => pdf::parse(filename, bytes),
        "epub" => epub::parse(filename, bytes),
        "htm" | "html" => html::parse(filename, bytes),
        "docx" => docx::parse(filename, bytes),
        "md" | "markdown" => parse_markdown(filename, bytes),
        "txt" | "" => parse_plain(filename, bytes),
        other => Err(anyhow!("unsupported file extension: {other}")),
    }
}

fn parse_markdown(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    let text =
        String::from_utf8(bytes.to_vec()).map_err(|e| anyhow!("file is not valid UTF-8: {e}"))?;
    let title = first_line_title(&text, filename);
    Ok(ParsedDocument {
        title,
        text,
        source_kind: SourceKind::Markdown,
        metadata: BTreeMap::new(),
    })
}

fn parse_plain(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    let text =
        String::from_utf8(bytes.to_vec()).map_err(|e| anyhow!("file is not valid UTF-8: {e}"))?;
    let title = first_line_title(&text, filename);
    Ok(ParsedDocument {
        title,
        text,
        source_kind: SourceKind::Plain,
        metadata: BTreeMap::new(),
    })
}

pub(crate) fn first_line_title(text: &str, fallback_filename: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim().trim_start_matches('#').trim();
        if !trimmed.is_empty() {
            return trimmed.chars().take(120).collect();
        }
    }
    Path::new(fallback_filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string()
}
