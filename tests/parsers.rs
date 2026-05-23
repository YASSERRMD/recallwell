//! Smoke tests for the document parsers.

use pagebridge::SourceKind;
use recallwell::ingest::parse_bytes;

#[test]
fn markdown_passthrough() {
    let md = b"# Hello\n\nThis is some text.\n";
    let parsed = parse_bytes("notes.md", md).expect("parse md");
    assert!(matches!(parsed.source_kind, SourceKind::Markdown));
    assert!(parsed.text.contains("This is some text"));
    assert_eq!(parsed.title, "Hello");
}

#[test]
fn plain_text_passthrough() {
    let txt = b"Some plain text.\nSecond line.";
    let parsed = parse_bytes("note.txt", txt).expect("parse txt");
    assert!(matches!(parsed.source_kind, SourceKind::Plain));
    assert!(parsed.text.starts_with("Some plain text"));
}

#[test]
fn html_extracts_main_content() {
    let html = br#"<!doctype html><html><head><title>The Title</title></head>
<body>
<nav>skip me</nav>
<main>
<h1>Main heading</h1>
<p>The body of the article.</p>
</main>
<footer>skip me too</footer>
</body></html>"#;
    let parsed = parse_bytes("page.html", html).expect("parse html");
    assert!(matches!(parsed.source_kind, SourceKind::Markdown));
    assert!(parsed.text.contains("Main heading"));
    assert!(parsed.text.contains("body of the article"));
    assert!(!parsed.text.contains("skip me"));
}

#[test]
fn unsupported_extension_rejected() {
    let bin = b"\x00\x01\x02";
    let err = parse_bytes("weird.xyz", bin).unwrap_err();
    assert!(err.to_string().contains("unsupported"));
}
