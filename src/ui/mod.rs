//! Static assets and HTML templates embedded into the binary.

pub static HTMX_JS: &[u8] = include_bytes!("assets/htmx.min.js");
pub static ALPINE_JS: &[u8] = include_bytes!("assets/alpine.min.js");
pub static RECALLWELL_CSS: &[u8] = include_bytes!("assets/recallwell.css");
pub static FAVICON_SVG: &[u8] = include_bytes!("assets/favicon.svg");
/// Small in-app version of the MY diamond logo (192x192, ~30 KB).
/// The full-resolution doc-grade logo lives in docs/assets/brand/.
pub static MY_LOGO_PNG: &[u8] = include_bytes!("assets/brand/my-logo-96.png");

pub static INDEX_HTML: &str = include_str!("templates/index.html");

/// Look up an embedded asset by basename or sub-path.
///
/// Returns `(bytes, content_type)` if known, `None` otherwise.
pub fn serve_asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    match path {
        "htmx.min.js" => Some((HTMX_JS, "application/javascript; charset=utf-8")),
        "alpine.min.js" => Some((ALPINE_JS, "application/javascript; charset=utf-8")),
        "recallwell.css" => Some((RECALLWELL_CSS, "text/css; charset=utf-8")),
        "favicon.svg" => Some((FAVICON_SVG, "image/svg+xml")),
        "brand/my-logo.png" => Some((MY_LOGO_PNG, "image/png")),
        _ => None,
    }
}

/// Render the index page with placeholders substituted.
pub fn render_index(token: &str, library_name: &str) -> String {
    INDEX_HTML
        .replace("{TOKEN}", token)
        .replace("{LIBRARY_NAME}", library_name)
        .replace("{VERSION}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_are_embedded() {
        assert!(!HTMX_JS.is_empty());
        assert!(!ALPINE_JS.is_empty());
        assert!(!RECALLWELL_CSS.is_empty());
        assert!(!FAVICON_SVG.is_empty());
        assert!(!MY_LOGO_PNG.is_empty());
        assert!(!INDEX_HTML.is_empty());
    }

    #[test]
    fn serve_asset_known_names() {
        assert!(serve_asset("htmx.min.js").is_some());
        assert!(serve_asset("alpine.min.js").is_some());
        assert!(serve_asset("recallwell.css").is_some());
        assert!(serve_asset("favicon.svg").is_some());
        assert!(serve_asset("brand/my-logo.png").is_some());
    }

    #[test]
    fn serve_asset_unknown_returns_none() {
        assert!(serve_asset("evil.exe").is_none());
        assert!(serve_asset("../etc/passwd").is_none());
        assert!(serve_asset("tailwind.min.js").is_none());
    }

    #[test]
    fn render_index_substitutes_placeholders() {
        let html = render_index("TOK", "reading");
        assert!(html.contains("TOK"));
        assert!(html.contains("reading"));
        assert!(html.contains(env!("CARGO_PKG_VERSION")));
        assert!(!html.contains("{TOKEN}"));
        assert!(!html.contains("{LIBRARY_NAME}"));
        assert!(!html.contains("{VERSION}"));
    }
}
