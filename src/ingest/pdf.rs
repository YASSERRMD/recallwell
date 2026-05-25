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
            // Pagebridge stores `preview(body, 120)` as the leaf's
            // routing_summary, which is the ONLY thing the BM25 index and
            // the navigator see. Prepend a search-friendly lead-in derived
            // from the chunk title so the preview is meaningful even when
            // pdf-extract glued the body into one unreadable run.
            let lead_in = lead_in_for(&chunk_title);
            if !lead_in.is_empty() {
                let _ = writeln!(md, "_{lead_in}_");
                md.push('\n');
            }
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

/// Split text into ~target-char chunks.
///
/// Strategy, applied in order:
///   1. Pre-segment the text by structural cues that pdf-extract often
///      preserves even when paragraph breaks are gone — ALL-CAPS section
///      headers (SKILLS, EXPERIENCE, ABOUT ME, ...) and year-range markers
///      (2024-2025, 2025-Present, ...). These are the CV / resume case.
///   2. Group adjacent segments into ~target-char chunks at paragraph
///      boundaries when possible.
///   3. If any chunk is still bigger than MAX, force-split it at sentence
///      boundaries so no leaf is ever monstrous.
fn chunk_into_leaves(text: &str) -> Vec<String> {
    // Step 1: pre-segment. We insert a sentinel "\n\n" before every detected
    // structural cue, so the existing paragraph-grouping logic below works
    // unchanged on the result.
    let pre = insert_structural_breaks(text);

    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();

    for para in pre.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        let starts_new_section = is_structural_boundary(para);

        if current.is_empty() {
            current.push_str(para);
        } else if starts_new_section {
            // Always start a new chunk at a structural break — that's the
            // whole point of inserting it (each section gets its own leaf).
            out.push(std::mem::take(&mut current));
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

    // Step 3: hard-split anything still over MAX_LEAF_CHARS so we never
    // produce a multi-KB leaf the LLM will summarise as just the first line.
    let mut split: Vec<String> = Vec::new();
    for c in out {
        if c.len() <= MAX_LEAF_CHARS {
            split.push(c);
        } else {
            split.extend(hard_split_long(&c, TARGET_LEAF_CHARS));
        }
    }

    if split.is_empty() {
        split.push(text.to_string());
    }
    split
}

/// Known section keywords (mostly CVs / resumes / reports). When we see one
/// of these as a substring on a word boundary, we force a section break
/// before it, even if it's glued to the previous word (as pdf-extract often
/// does on multi-column or absolute-positioned PDFs).
const SECTION_KEYWORDS: &[&str] = &[
    "ABOUT ME",
    "ABOUT",
    "PROFILE",
    "OBJECTIVE",
    "SUMMARY",
    "EDUCATION",
    "EXPERIENCE",
    "WORK EXPERIENCE",
    "PROFESSIONAL EXPERIENCE",
    "EMPLOYMENT",
    "SKILLS",
    "TECHNICAL SKILLS",
    "SOFT SKILLS",
    "SOFTWARE KNOWLEDGE",
    "LANGUAGES",
    "REFERENCES",
    "CERTIFICATIONS",
    "CERTIFICATES",
    "AWARDS",
    "PROJECTS",
    "PUBLICATIONS",
    "INTERESTS",
    "ACHIEVEMENTS",
    "PERSONAL DETAILS",
    "CONTACT",
    "SIGNATURE",
    "DECLARATION",
];

/// Insert blank-line separators before "structural" cues that pdf-extract
/// often preserves even when paragraph breaks are stripped:
///   - Known section keywords (SKILLS, EXPERIENCE, ...) anywhere in the text
///   - Year-range markers like 2023-2024, 2025-Present.
///
/// Keywords are recognised even when glued to surrounding text — e.g.
/// "SKILLSEXPERIENCE2025-Present" is split into three pieces. This is the
/// common case in CV PDFs where pdf-extract loses whitespace between
/// absolute-positioned columns.
fn insert_structural_breaks(text: &str) -> String {
    let mut break_points: Vec<usize> = Vec::new();

    // ---- pass 1: section keywords ----
    // Sort keywords by length descending so we try "PROFESSIONAL EXPERIENCE"
    // before "EXPERIENCE", "ABOUT ME" before "ABOUT", etc.
    let mut kws: Vec<&str> = SECTION_KEYWORDS.to_vec();
    kws.sort_by_key(|k| std::cmp::Reverse(k.len()));

    let bytes = text.as_bytes();
    let mut i = 0;
    let mut consumed_until = 0usize;
    while i < bytes.len() {
        for kw in &kws {
            let kbytes = kw.as_bytes();
            if i + kbytes.len() <= bytes.len() && &bytes[i..i + kbytes.len()] == kbytes {
                // Word-boundary on the LEFT: previous char must be non-uppercase-letter
                // (so we split "SKILLSEXPERIENCE" into SKILLS|EXPERIENCE because
                //  the char before "EXPERIENCE" is 'S' uppercase => we DO break;
                //  but a normal word boundary is also fine).
                let left_ok = i == 0 || {
                    let prev = bytes[i - 1];
                    !prev.is_ascii_uppercase() || prev == b' '
                };
                // Word-boundary on the RIGHT: next char should not extend the keyword
                // into a longer ALL-CAPS word that means something else.
                let next_idx = i + kbytes.len();
                let right_ok = next_idx >= bytes.len() || {
                    let next = bytes[next_idx];
                    // Allow break if followed by uppercase (another section start),
                    // lowercase (body text), digit, space, or punctuation.
                    next.is_ascii_alphanumeric() || next.is_ascii_whitespace() || next.is_ascii_punctuation()
                };
                if (left_ok || i > consumed_until) && right_ok {
                    break_points.push(i);
                    consumed_until = i + kbytes.len();
                    i += kbytes.len();
                    break;
                }
            }
        }
        if i < consumed_until {
            continue;
        }
        i += 1;
    }

    // ---- pass 2: year ranges ----
    let mut j = 0;
    while j + 5 < bytes.len() {
        if bytes[j..j + 4].iter().all(|b| b.is_ascii_digit())
            && bytes[j + 4] == b'-'
            && (1900..2200).contains(&parse_4digit(&bytes[j..j + 4]))
            && (j == 0 || !bytes[j - 1].is_ascii_digit())
        {
            break_points.push(j);
            j += 5;
        } else {
            j += 1;
        }
    }

    break_points.sort_unstable();
    break_points.dedup();

    // Apply breaks.
    let mut out = String::with_capacity(text.len() + break_points.len() * 2);
    let mut cursor = 0;
    for bp in break_points {
        if bp <= cursor {
            continue;
        }
        out.push_str(&text[cursor..bp]);
        if !out.ends_with("\n\n") && !out.is_empty() {
            out.push_str("\n\n");
        }
        cursor = bp;
    }
    out.push_str(&text[cursor..]);
    out
}

fn is_ascii_digit_at(bytes: &[u8], i: usize) -> bool {
    bytes.get(i).is_some_and(u8::is_ascii_digit)
}

fn parse_4digit(b: &[u8]) -> u32 {
    let mut n = 0u32;
    for &c in b.iter().take(4) {
        n = n * 10 + u32::from(c - b'0');
    }
    n
}

/// Hard-split a string into ~target-char chunks at sentence ('. ') or
/// fallback to space boundaries.
fn hard_split_long(s: &str, target: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut end = (i + target).min(bytes.len());
        // Walk back to a sentence end or space boundary
        if end < bytes.len() {
            let lookback_start = end.saturating_sub(target / 2);
            let chunk = &s[i..end];
            if let Some(pos) = chunk.rfind(". ") {
                if i + pos + 2 > lookback_start {
                    end = i + pos + 2;
                }
            } else if let Some(pos) = chunk.rfind(' ') {
                if i + pos + 1 > lookback_start {
                    end = i + pos + 1;
                }
            }
        }
        // Find a valid char boundary at or before `end`.
        let mut k = end;
        while k > i && !s.is_char_boundary(k) {
            k -= 1;
        }
        out.push(s[i..k].to_string());
        i = k;
    }
    out
}

/// Pick a short title for a chunk. Prefer a leading numbered heading like
/// "3. Scope" or "3.1 Solution..."; otherwise use the first sentence.
/// Build a short descriptive sentence that names what kind of section this
/// chunk is, using vocabulary an asking user is likely to type. This becomes
/// the first 120 chars of the leaf body, which is the only thing pagebridge's
/// BM25 index and navigation step actually see.
///
/// Examples:
///   "SKILLS"               -> "Section: SKILLS. Skills, abilities, ..."
///   "EXPERIENCE"           -> "Section: EXPERIENCE. Work history, roles, ..."
///   "EDUCATION"            -> "Section: EDUCATION. Degree, school, ..."
///   "2024-2025 Worked..."  -> "Section: role / job at 2024-2025. ..."
///   numbered headings      -> "Section: 3. Scope. Subsection content."
fn lead_in_for(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    let keywords: &[(&str, &str)] = &[
        ("skills", "Skills, abilities, competencies, expertise, and technical strengths."),
        ("experience", "Work experience, roles, employment history, jobs held, and responsibilities."),
        ("employment", "Employment history, jobs, roles, and positions held."),
        ("education", "Education, degree, university, school, academic background, qualifications."),
        ("languages", "Languages spoken, written, fluency, and linguistic abilities."),
        ("references", "Professional references and contacts."),
        ("certifications", "Certifications, certificates, courses, and professional credentials."),
        ("certificates", "Certifications, certificates, courses, and professional credentials."),
        ("awards", "Awards, recognition, honors, and achievements."),
        ("projects", "Projects, work samples, portfolio, and notable deliverables."),
        ("publications", "Publications, papers, articles, and written work."),
        ("interests", "Interests, hobbies, and personal pursuits."),
        ("achievements", "Achievements, accomplishments, and recognition."),
        ("personal", "Personal details, biographical information, date of birth, nationality."),
        ("contact", "Contact information, email, phone, and address."),
        ("about", "About me, profile, professional summary, and biography."),
        ("profile", "Profile, professional summary, biography, and overview."),
        ("objective", "Career objective, professional goals, and aspirations."),
        ("summary", "Professional summary, career overview, and qualifications snapshot."),
    ];
    for (k, lead) in keywords {
        if lower.contains(k) {
            return format!("Section about {k}. {lead}");
        }
    }
    // Year-range chunks like "2024-2025 Worked as..." — likely a job entry.
    if title.len() >= 9 && looks_like_year_range_start(title) {
        return format!("Section about a job or role during {}.",
            title.chars().take(9).collect::<String>());
    }
    String::new()
}

fn chunk_title_for(chunk: &str, page_no: usize, chunk_idx: usize) -> String {
    let first_line = chunk.lines().next().unwrap_or("").trim();
    // Numbered section pattern: "3. Scope of Advisory Support" or "3.1 ..."
    if first_line.len() <= 120 && looks_like_heading(first_line) {
        return first_line.to_string();
    }
    // ALL-CAPS section header (CV-style: SKILLS, EXPERIENCE, EDUCATION).
    if first_line.len() <= 60 && looks_like_caps_header(first_line) {
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

/// Does this paragraph's leading content look like the start of a new
/// distinct section that should always live in its own leaf?
fn is_structural_boundary(para: &str) -> bool {
    let first_line = para.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return false;
    }
    // ALL-CAPS section header at the start.
    if looks_like_caps_header(first_line) {
        return true;
    }
    // Year-range marker at the start: "2024-2025", "2025-Present", etc.
    looks_like_year_range_start(first_line)
}

fn looks_like_year_range_start(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 6 {
        return false;
    }
    if !bytes[..4].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let year = parse_4digit(&bytes[..4]);
    if !(1900..2200).contains(&year) {
        return false;
    }
    bytes[4] == b'-'
}

fn looks_like_caps_header(line: &str) -> bool {
    let letters: Vec<char> = line.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if letters.len() < 4 || letters.len() > 30 {
        return false;
    }
    letters.iter().all(|c| c.is_ascii_uppercase())
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

    #[test]
    fn detects_caps_header() {
        assert!(looks_like_caps_header("SKILLS"));
        assert!(looks_like_caps_header("EXPERIENCE"));
        assert!(looks_like_caps_header("ABOUT ME"));
        assert!(looks_like_caps_header("PERSONAL DETAILS"));
        assert!(!looks_like_caps_header("Skills"));
        assert!(!looks_like_caps_header("Md Hasibur Rahman"));
    }

    #[test]
    fn structural_breaks_split_concatenated_cv() {
        // A simulation of pdf-extract's CV output: section headers and
        // year-ranges glued together with no whitespace.
        let cv = "Lead of Software Quality AssuranceABOUT MEAs a QA professional with 3 years experienceSKILLSTest planning, defect identification, QA process managementEXPERIENCE2025-PresentLed QA teams at Codevioso2024-2025Junior Officer at Sheba.xyzEDUCATIONB.Sc Computer Science 2018-2023";
        let chunks = chunk_into_leaves(cv);
        assert!(chunks.len() >= 3, "expected 3+ chunks, got {}", chunks.len());
        let all = chunks.join("\n\n");
        assert!(all.contains("ABOUT ME"));
        assert!(all.contains("SKILLS"));
        assert!(all.contains("EXPERIENCE"));
        assert!(all.contains("EDUCATION"));
    }

    #[test]
    fn hard_split_caps_giant_chunks() {
        let big = "x".repeat(5000);
        let chunks = chunk_into_leaves(&big);
        for c in &chunks {
            assert!(c.len() <= MAX_LEAF_CHARS + 10, "chunk too big: {}", c.len());
        }
    }
}
