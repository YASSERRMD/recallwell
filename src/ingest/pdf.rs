//! PDF parser using the pure-Rust `pdf-extract` crate.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, Result};
use pagebridge::SourceKind;

use crate::ingest::{first_line_title, ParsedDocument};

pub fn parse(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    // pdf-extract returns extracted text from all pages concatenated.
    // It is CPU-bound; callers should run this inside `spawn_blocking`.
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| anyhow!("pdf extract failed: {e}"))?;
    if text.trim().is_empty() {
        return Err(anyhow!(
            "PDF appears to contain no extractable text (scanned PDF? OCR is a v0.2 feature)"
        ));
    }
    let title = pdf_title_or_filename(&text, filename);

    let mut metadata = BTreeMap::new();
    metadata.insert("format".into(), "pdf".into());
    metadata.insert(
        "original_filename".into(),
        Path::new(filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(filename)
            .to_string(),
    );

    Ok(ParsedDocument {
        title,
        text,
        source_kind: SourceKind::Pdf,
        metadata,
    })
}

fn pdf_title_or_filename(text: &str, filename: &str) -> String {
    // Heuristic: first non-empty line within the first 30 lines whose length
    // looks like a heading (10..120 chars). Fall back to filename stem.
    for line in text.lines().take(30) {
        let trimmed = line.trim();
        let len = trimmed.chars().count();
        if (10..=120).contains(&len) {
            return trimmed.to_string();
        }
    }
    first_line_title(text, filename)
}
