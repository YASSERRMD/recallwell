//! PDF parser.
//!
//! Pre-extracts text via `pdf-extract`, then formats the result as Markdown
//! so pagebridge builds a useful tree:
//!   - `# {title}`                — document root
//!   - `## Page N`                — one section per pdf page
//!   - `### {chunk-title}`        — chunked sub-sections, one leaf each
//!
//! Pagebridge's markdown parser creates one leaf per section BODY, so without
//! the `###` sub-sections a multi-page PDF collapses to a single giant leaf
//! and BM25 can no longer find anything specific. We chunk roughly every
//! ~700 chars at paragraph boundaries.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{anyhow, Result};
use pagebridge::SourceKind;

use crate::ingest::{first_line_title, ParsedDocument};

const TARGET_LEAF_CHARS: usize = 1400;
const MAX_LEAF_CHARS: usize = 2500;

pub fn parse(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    let extracted = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| anyhow!("pdf extract failed: {e}"))?;
    if extracted.trim().is_empty() {
        return Err(anyhow!(
            "PDF appears to contain no extractable text (scanned PDF? OCR is on the v0.2 roadmap)"
        ));
    }

    let title = pdf_title_or_filename(&extracted, filename);
    let pages: Vec<&str> = extracted.split('\u{000c}').collect();

    let mut md = String::with_capacity(extracted.len() + 1024);
    let _ = writeln!(md, "# {title}");
    md.push('\n');

    let mut non_empty_pages = 0u32;
    let mut total_leaves = 0u32;

    for (page_idx, page_text) in pages.iter().enumerate() {
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        non_empty_pages += 1;
        let page_no = page_idx + 1;
        let _ = writeln!(md, "\n## Page {page_no}\n");

        for (chunk_idx, chunk) in chunk_into_leaves(trimmed).into_iter().enumerate() {
            total_leaves += 1;
            let chunk_title = chunk_title_for(&chunk, page_no, chunk_idx + 1);
            let _ = writeln!(md, "### {chunk_title}\n");
            md.push_str(chunk.trim());
            md.push('\n');
            md.push('\n');
        }
    }

    let mut metadata = BTreeMap::new();
    metadata.insert("format".into(), "pdf".into());
    metadata.insert("pages".into(), non_empty_pages.to_string());
    metadata.insert("leaves".into(), total_leaves.to_string());
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
        raw: md.into_bytes(),
        source_kind: SourceKind::Markdown,
        metadata,
    })
}

/// Split text into ~700-char chunks at paragraph boundaries. Paragraphs are
/// runs of lines separated by blank lines.
fn chunk_into_leaves(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();

    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        if current.is_empty() {
            current.push_str(para);
        } else if current.len() + para.len() + 2 <= MAX_LEAF_CHARS
            && current.len() < TARGET_LEAF_CHARS
        {
            current.push_str("\n\n");
            current.push_str(para);
        } else {
            out.push(std::mem::take(&mut current));
            current.push_str(para);
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        // Fallback: emit the whole text as one chunk so we never produce a
        // page section with zero sub-sections.
        out.push(text.to_string());
    }
    out
}

/// Pick a short title for a chunk. Prefer a leading numbered heading like
/// "3. Scope" or "3.1 Solution..."; otherwise use the first sentence.
fn chunk_title_for(chunk: &str, page_no: usize, chunk_idx: usize) -> String {
    let first_line = chunk.lines().next().unwrap_or("").trim();
    // Numbered section pattern: "3. Scope of Advisory Support" or "3.1 ..."
    if first_line.len() <= 120 && looks_like_heading(first_line) {
        return first_line.to_string();
    }
    // Otherwise: first sentence, capped.
    let mut snippet = first_line.to_string();
    if let Some(stop) = first_line.find(". ") {
        snippet = first_line[..stop].to_string();
    }
    let snippet = snippet.chars().take(80).collect::<String>();
    if snippet.is_empty() {
        format!("Page {page_no}, section {chunk_idx}")
    } else {
        snippet
    }
}

fn looks_like_heading(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    // Allow "N." or "N.N" prefix
    if bytes.first().is_some_and(u8::is_ascii_digit) {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
            }
        }
        // Need a space and then a capital letter
        if i < bytes.len() && bytes[i] == b' ' {
            i += 1;
            if i < bytes.len() && bytes[i].is_ascii_uppercase() {
                return true;
            }
        }
    }
    false
}

fn pdf_title_or_filename(text: &str, filename: &str) -> String {
    for line in text.lines().take(30) {
        let trimmed = line.trim();
        let len = trimmed.chars().count();
        if (10..=120).contains(&len) {
            return trimmed.to_string();
        }
    }
    first_line_title(text, filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_short_doc_into_one_leaf() {
        let leaves = chunk_into_leaves("hello world");
        assert_eq!(leaves.len(), 1);
    }

    #[test]
    fn chunks_groups_paragraphs_under_target() {
        let text = "para1\n\npara2\n\npara3";
        let leaves = chunk_into_leaves(text);
        assert_eq!(leaves.len(), 1);
        assert!(leaves[0].contains("para1") && leaves[0].contains("para3"));
    }

    #[test]
    fn chunks_splits_when_over_target() {
        let big = "x".repeat(800);
        let text = format!("{big}\n\n{big}\n\n{big}");
        let leaves = chunk_into_leaves(&text);
        assert!(leaves.len() >= 2);
    }

    #[test]
    fn detects_numbered_heading() {
        assert!(looks_like_heading("3. Scope of Advisory Support"));
        assert!(looks_like_heading("3.1 Solution and System Architecture"));
        assert!(looks_like_heading("12. Conclusion"));
        assert!(!looks_like_heading("Just a regular sentence."));
        assert!(!looks_like_heading("3rd party libraries"));
    }
}
