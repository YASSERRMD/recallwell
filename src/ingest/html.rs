//! HTML parser: extracts main content and renders it as markdown.

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use pagebridge::SourceKind;
use scraper::{ElementRef, Html, Node, Selector};

use crate::ingest::ParsedDocument;

pub fn parse(filename: &str, bytes: &[u8]) -> Result<ParsedDocument> {
    let html = std::str::from_utf8(bytes).map_err(|e| anyhow!("html utf-8: {e}"))?;
    let doc = Html::parse_document(html);

    let title_sel = Selector::parse("title").unwrap();
    let title = doc
        .select(&title_sel)
        .next()
        .map(|t| t.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            std::path::Path::new(filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string()
        });

    let main = pick_main(&doc).unwrap_or_else(|| doc.root_element());
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    render_node(&mut out, main, 0);

    let mut metadata = BTreeMap::new();
    metadata.insert("format".into(), "html".into());

    Ok(ParsedDocument {
        title,
        text: out.trim().to_string(),
        source_kind: SourceKind::Markdown,
        metadata,
    })
}

fn pick_main(doc: &Html) -> Option<ElementRef<'_>> {
    for sel in &[
        "main",
        "article",
        "[role=\"main\"]",
        "#content",
        "#main",
        ".post-content",
        ".article-content",
    ] {
        if let Ok(selector) = Selector::parse(sel) {
            if let Some(el) = doc.select(&selector).next() {
                return Some(el);
            }
        }
    }
    if let Ok(body) = Selector::parse("body") {
        return doc.select(&body).next();
    }
    None
}

const STRIP_TAGS: &[&str] = &["script", "style", "nav", "footer", "aside", "header"];

pub(crate) fn render_node(out: &mut String, element: ElementRef<'_>, depth: usize) {
    let tag = element.value().name();
    if STRIP_TAGS.contains(&tag) {
        return;
    }

    match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level: usize = tag[1..].parse().unwrap_or(1);
            out.push('\n');
            for _ in 0..level {
                out.push('#');
            }
            out.push(' ');
            out.push_str(&clean_text(&inner_text(element)));
            out.push('\n');
            out.push('\n');
        }
        "p" => {
            let text = clean_text(&inner_text(element));
            if !text.is_empty() {
                out.push_str(&text);
                out.push('\n');
                out.push('\n');
            }
        }
        "li" => {
            out.push_str("- ");
            out.push_str(&clean_text(&inner_text(element)));
            out.push('\n');
        }
        "blockquote" => {
            for line in inner_text(element).lines() {
                let l = line.trim();
                if !l.is_empty() {
                    out.push_str("> ");
                    out.push_str(l);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        "code" if depth > 0 => {
            out.push('`');
            out.push_str(&inner_text(element));
            out.push('`');
        }
        "pre" => {
            out.push_str("\n```\n");
            out.push_str(&inner_text(element));
            out.push_str("\n```\n\n");
        }
        "br" => {
            out.push('\n');
        }
        _ => {
            for child in element.children() {
                match child.value() {
                    Node::Element(_) => {
                        if let Some(el) = ElementRef::wrap(child) {
                            render_node(out, el, depth + 1);
                        }
                    }
                    Node::Text(t) => {
                        let text = t.text.to_string();
                        let cleaned = clean_text(&text);
                        if !cleaned.is_empty() {
                            out.push_str(&cleaned);
                            out.push(' ');
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn inner_text(element: ElementRef<'_>) -> String {
    element.text().collect::<Vec<_>>().join(" ")
}

fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}
