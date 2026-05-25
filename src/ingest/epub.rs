//! EPUB parser: concatenates chapters as markdown.

use std::collections::BTreeMap;
use std::io::Cursor;

use anyhow::{anyhow, Result};
use epub::doc::EpubDoc;
use pagebridge::SourceKind;

use crate::ingest::ParsedDocument;

pub fn parse(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut doc = EpubDoc::from_reader(cursor).map_err(|e| anyhow!("epub open: {e}"))?;

    let title = doc
        .mdata("title")
        .map(extract_metadata_value)
        .unwrap_or_else(|| epub_filename_title(filename));
    let author = doc
        .mdata("creator")
        .map(extract_metadata_value)
        .unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    if !author.is_empty() {
        out.push_str(&format!("*by {author}*\n\n"));
    }

    let spine_len = doc.spine.len();
    for _ in 0..spine_len {
        if let Some((content_string, _mime)) = doc.get_current_str() {
            let chapter_text = html_to_markdown(content_string.as_bytes())?;
            if !chapter_text.trim().is_empty() {
                out.push_str(&chapter_text);
                out.push_str("\n\n");
            }
        }
        if !doc.go_next() {
            break;
        }
    }

    let mut metadata = BTreeMap::new();
    metadata.insert("format".into(), "epub".into());
    if !author.is_empty() {
        metadata.insert("author".into(), author);
    }

    Ok(ParsedDocument {
        title,
        raw: out.into_bytes(),
        source_kind: SourceKind::Markdown,
        metadata,
    })
}

fn extract_metadata_value(item: impl std::fmt::Debug) -> String {
    // The MetadataItem type's Debug impl exposes its content; for the simple
    // value we want we fall back to its string formatting and pick the value
    // bit. This is intentionally lenient because epub metadata schemas vary.
    let debug = format!("{item:?}");
    // Try to find "value: \"...\"" first; otherwise pass through.
    if let Some(start) = debug.find("value: \"") {
        let rest = &debug[start + 8..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    debug
}

fn epub_filename_title(filename: &str) -> String {
    std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string()
}

fn html_to_markdown(bytes: &[u8]) -> Result<String> {
    let html = std::str::from_utf8(bytes).map_err(|e| anyhow!("epub chapter utf-8: {e}"))?;
    let doc = scraper::Html::parse_document(html);
    let body_sel = scraper::Selector::parse("body").unwrap();
    let body = doc.select(&body_sel).next();
    let mut out = String::new();
    if let Some(b) = body {
        crate::ingest::html::render_node(&mut out, b, 0);
    } else {
        out.push_str(&doc.root_element().text().collect::<String>());
    }
    Ok(out.trim().to_string())
}
