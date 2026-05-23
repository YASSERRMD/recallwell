//! DOCX parser: extracts paragraphs and headings as markdown.

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use docx_rs::{read_docx, DocumentChild, ParagraphChild, RunChild};
use pagebridge::SourceKind;

use crate::ingest::{first_line_title, ParsedDocument};

pub fn parse(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    let doc = read_docx(bytes).map_err(|e| anyhow!("docx read: {e}"))?;
    let mut out = String::new();
    for child in doc.document.children {
        match child {
            DocumentChild::Paragraph(p) => {
                let style = p.property.style.map(|s| s.val).unwrap_or_default();
                let mut text = String::new();
                for c in p.children {
                    if let ParagraphChild::Run(run) = c {
                        for rc in run.children {
                            if let RunChild::Text(t) = rc {
                                text.push_str(&t.text);
                            }
                        }
                    }
                }
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                let heading_level = heading_level_for(&style);
                if let Some(level) = heading_level {
                    out.push('\n');
                    for _ in 0..level {
                        out.push('#');
                    }
                    out.push(' ');
                    out.push_str(text);
                    out.push('\n');
                    out.push('\n');
                } else {
                    out.push_str(text);
                    out.push('\n');
                    out.push('\n');
                }
            }
            DocumentChild::Table(_) => {
                // For v0.1 we ignore tables; v0.2 may render as markdown tables.
            }
            _ => {}
        }
    }
    let title = first_line_title(&out, filename);
    let mut metadata = BTreeMap::new();
    metadata.insert("format".into(), "docx".into());

    Ok(ParsedDocument {
        title,
        text: out.trim().to_string(),
        source_kind: SourceKind::Markdown,
        metadata,
    })
}

fn heading_level_for(style: &str) -> Option<usize> {
    let lower = style.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("heading") {
        let rest = rest.trim();
        if rest.is_empty() {
            return Some(1);
        }
        return rest.parse::<usize>().ok().filter(|n| (1..=6).contains(n));
    }
    if lower == "title" {
        return Some(1);
    }
    None
}
