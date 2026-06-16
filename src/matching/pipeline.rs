use serde::{Deserialize, Serialize};

use crate::matching::exact::find_exact_match;
use crate::matching::fuzzy::{
    build_paragraphs, find_fuzzy_match, find_fuzzy_match_windowed,
    find_fuzzy_match_with_score, find_fuzzy_match_windowed_with_score,
};
use crate::matching::normalize::{normalize, ocr_normalize};
use crate::matching::page_scope::{
    build_paragraphs_ocr, find_page_ranges, page_scoped_line_range, PageRange,
};

/// Find the line index where `needle` best matches within `lines`.
/// Tries exact substring match first, then single-paragraph fuzzy,
/// then multi-paragraph windowed fuzzy matching.
/// Returns `None` if the needle is empty or no match meets the threshold.
pub fn find_match_line(needle: &str, lines: &[&str], threshold: f64) -> Option<usize> {
    let norm_needle = normalize(needle);
    if norm_needle.is_empty() {
        return None;
    }

    // 1. Exact substring match (fast path)
    if let Some(line_idx) = find_exact_match(needle, lines) {
        return Some(line_idx);
    }

    // 2. Fuzzy: single-paragraph
    let paragraphs = build_paragraphs(lines);
    if let Some(line_idx) = find_fuzzy_match(needle, &paragraphs, threshold) {
        return Some(line_idx);
    }

    // 3. Windowed fuzzy (multi-paragraph spans, sizes 2-3)
    find_fuzzy_match_windowed(needle, &paragraphs, threshold)
}

/// Metadata about how a match was found, used by the preview command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatchMetadata {
    pub line_idx: usize,
    pub match_type: String,
    pub confidence: f64,
}

/// Find the line index where `needle` best matches within `lines`,
/// optionally scoped to a page range.
///
/// Matching pipeline:
/// 1. OCR-normalize both needle and lines
/// 2. If page_label + page_ranges provided, restrict to +-1 page
/// 3. Try exact/fuzzy/windowed match within scope
/// 4. If scoped search failed and scope was restricted, retry on full document
pub fn find_match_line_scoped(
    needle: &str,
    lines: &[&str],
    threshold: f64,
    page_label: Option<&str>,
    page_ranges: &[PageRange],
) -> Option<usize> {
    find_match_line_scoped_with_metadata(needle, lines, threshold, page_label, page_ranges)
        .map(|m| m.line_idx)
}

/// Like `find_match_line_scoped` but returns rich metadata about the match.
/// Tries the same pipeline (exact -> fuzzy -> windowed -> OCR) and records
/// which stage succeeded plus the score.
pub fn find_match_line_scoped_with_metadata(
    needle: &str,
    lines: &[&str],
    threshold: f64,
    page_label: Option<&str>,
    page_ranges: &[PageRange],
) -> Option<MatchMetadata> {
    let ocr_needle = ocr_normalize(needle);
    if ocr_needle.is_empty() {
        return None;
    }

    let ocr_lines: Vec<String> = lines.iter().map(|l| ocr_normalize(l)).collect();
    let ocr_line_refs: Vec<&str> = ocr_lines.iter().map(|s| s.as_str()).collect();

    let scope = page_label.and_then(|pl| page_scoped_line_range(pl, page_ranges));

    // Try scoped search first
    if let Some((start, end)) = scope {
        let end = end.min(ocr_line_refs.len().saturating_sub(1));
        if start <= end {
            let scoped_lines = &ocr_line_refs[start..=end];
            if let Some(meta) = find_match_with_metadata_inner(&ocr_needle, scoped_lines, threshold) {
                return Some(MatchMetadata {
                    line_idx: start + meta.line_idx,
                    match_type: meta.match_type,
                    confidence: meta.confidence,
                });
            }
        }
    }

    // Fall back to full-document search
    find_match_with_metadata_inner(&ocr_needle, &ocr_line_refs, threshold)
}

/// Inner function that tries the matching pipeline and returns metadata.
fn find_match_with_metadata_inner(
    needle: &str,
    lines: &[&str],
    threshold: f64,
) -> Option<MatchMetadata> {
    let norm_needle = normalize(needle);
    if norm_needle.is_empty() {
        return None;
    }

    // 1. Exact substring match
    if let Some(line_idx) = find_exact_match(needle, lines) {
        return Some(MatchMetadata {
            line_idx,
            match_type: "exact".to_string(),
            confidence: 1.0,
        });
    }

    // 2. Fuzzy: single-paragraph
    let paragraphs = build_paragraphs(lines);
    if let Some((line_idx, score)) = find_fuzzy_match_with_score(needle, &paragraphs, threshold) {
        return Some(MatchMetadata {
            line_idx,
            match_type: "fuzzy".to_string(),
            confidence: score,
        });
    }

    // 3. Windowed fuzzy
    if let Some((line_idx, score)) = find_fuzzy_match_windowed_with_score(needle, &paragraphs, threshold) {
        return Some(MatchMetadata {
            line_idx,
            match_type: "fuzzy_windowed".to_string(),
            confidence: score,
        });
    }

    // 4. OCR-enhanced paragraph matching with hyphen-rejoin
    let ocr_paragraphs = build_paragraphs_ocr(lines);
    if let Some((line_idx, score)) = find_fuzzy_match_with_score(needle, &ocr_paragraphs, threshold) {
        return Some(MatchMetadata {
            line_idx,
            match_type: "fuzzy_ocr".to_string(),
            confidence: score,
        });
    }

    if let Some((line_idx, score)) = find_fuzzy_match_windowed_with_score(needle, &ocr_paragraphs, threshold) {
        return Some(MatchMetadata {
            line_idx,
            match_type: "fuzzy_ocr_windowed".to_string(),
            confidence: score,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // find_match_line tests (integration)
    // -------------------------------------------------------------------

    #[test]
    fn test_find_match_line_exact_preferred() {
        let lines = vec![
            "The quick brown fox",
            "jumps over the lazy dog",
            "",
            "A completely different paragraph",
        ];
        assert_eq!(
            find_match_line("brown fox jumps over", &lines, 0.65),
            Some(1)
        );
    }

    #[test]
    fn test_find_match_line_fuzzy_fallback() {
        let lines = vec![
            "The quikc brownn fox",
            "jumps over the lazy dog",
            "",
            "Something else entirely",
        ];
        // Typos prevent exact match, but fuzzy should match first paragraph
        let result =
            find_match_line("quick brown fox jumps over the lazy", &lines, 0.4);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_find_match_line_empty_needle() {
        let lines = vec!["hello world"];
        assert_eq!(find_match_line("", &lines, 0.65), None);
        assert_eq!(find_match_line("   ", &lines, 0.65), None);
    }

    #[test]
    fn test_find_match_line_no_match() {
        let lines = vec!["alpha beta gamma", "", "delta epsilon zeta"];
        assert_eq!(
            find_match_line(
                "completely unrelated content that shares no substring",
                &lines,
                0.65
            ),
            None
        );
    }

    #[test]
    fn test_find_match_line_single_line_doc() {
        let lines = vec!["the only line in the document"];
        assert_eq!(find_match_line("only line", &lines, 0.65), Some(0));
    }

    #[test]
    fn test_find_match_line_windowed_integration() {
        // Test that find_match_line uses windowed matching when single-para fails
        let lines = vec![
            "end of first paragraph",
            "",
            "start of second paragraph",
        ];
        let result = find_match_line(
            "end of first paragraph start of second paragraph",
            &lines,
            0.5,
        );
        // Exact match won't work (text spans paragraphs). Single-para fuzzy won't
        // match well. Windowed should catch it.
        assert!(result.is_some(), "windowed match should find cross-paragraph span");
    }

    // -------------------------------------------------------------------
    // find_match_line_scoped tests
    // -------------------------------------------------------------------

    #[test]
    fn test_scoped_match_finds_correct_page() {
        // Markdown with page markers for 3 pages.
        // Text "important finding" appears on page 1 (OCR page 1).
        let lines = vec![
            "<!-- Page 0 - 0 images -->",
            "introduction text",
            "<!-- Page 1 - 0 images -->",
            "the important finding here",
            "<!-- Page 2 - 0 images -->",
            "conclusion text",
        ];
        let page_ranges = find_page_ranges(&lines);

        // Annotation with page_label="2" (OCR page 1)
        let result = find_match_line_scoped(
            "important finding",
            &lines,
            0.5,
            Some("2"),
            &page_ranges,
        );
        assert_eq!(result, Some(3));
    }

    #[test]
    fn test_scoped_ocr_normalized_match() {
        // Annotation has ligature, markdown has plain text
        let lines = vec![
            "<!-- Page 0 - 0 images -->",
            "the field of knowledge",
        ];
        let page_ranges = find_page_ranges(&lines);

        let result = find_match_line_scoped(
            "the \u{FB01}eld of knowledge",
            &lines,
            0.5,
            Some("1"),
            &page_ranges,
        );
        assert!(result.is_some(), "OCR-normalized ligature should match");
    }

    #[test]
    fn test_scoped_fallback_to_full_doc() {
        // page_label points to a page that doesn't exist -> fall back to full doc
        let lines = vec![
            "<!-- Page 0 - 0 images -->",
            "the important text is here",
        ];
        let page_ranges = find_page_ranges(&lines);

        let result = find_match_line_scoped(
            "important text",
            &lines,
            0.5,
            Some("99"), // page 99 doesn't exist
            &page_ranges,
        );
        assert!(result.is_some(), "should fall back to full-doc search");
    }

    #[test]
    fn test_scoped_no_page_label() {
        // When page_label is None, should search full document
        let lines = vec![
            "<!-- Page 0 - 0 images -->",
            "the text to find",
        ];
        let page_ranges = find_page_ranges(&lines);

        let result = find_match_line_scoped(
            "text to find",
            &lines,
            0.5,
            None,
            &page_ranges,
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_scoped_empty_needle() {
        let lines = vec!["some text"];
        assert_eq!(
            find_match_line_scoped("", &lines, 0.5, None, &[]),
            None
        );
    }

    #[test]
    fn test_scoped_cross_paragraph_match() {
        // Annotation spans two paragraphs within the same page
        let lines = vec![
            "<!-- Page 0 - 0 images -->",
            "end of first paragraph",
            "",
            "start of second paragraph",
            "",
            "unrelated trailing text",
        ];
        let page_ranges = find_page_ranges(&lines);

        // Use full-doc search (no page constraint) to focus on the windowed matching
        let result = find_match_line_scoped(
            "end of first paragraph start of second paragraph",
            &lines,
            0.5,
            None,
            &page_ranges,
        );
        assert!(result.is_some(), "should find cross-paragraph match via windowed");
    }

    #[test]
    fn test_scoped_hyphenation_rejoin() {
        // Annotation text with line-break hyphenation in the markdown.
        // After OCR line-level normalize + paragraph hyphen rejoin,
        // "the knowl-" + "edge base" becomes "the knowledge base".
        let lines = vec![
            "the knowl-",
            "edge base",
        ];
        let result = find_match_line_scoped(
            "the knowledge base",
            &lines,
            0.5,
            None,
            &[],
        );
        assert!(result.is_some(), "OCR hyphenation should be rejoined and matched");
    }

    // -------------------------------------------------------------------
    // MatchMetadata tests
    // -------------------------------------------------------------------

    #[test]
    fn test_match_metadata_serialization() {
        let meta = MatchMetadata {
            line_idx: 42,
            match_type: "fuzzy".to_string(),
            confidence: 0.85,
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["line_idx"], 42);
        assert_eq!(json["match_type"], "fuzzy");
        assert_eq!(json["confidence"], 0.85);
    }

    #[test]
    fn test_match_with_metadata_exact() {
        let lines = vec!["The quick brown fox jumps over the lazy dog"];
        let result = find_match_line_scoped_with_metadata(
            "brown fox", &lines, 0.65, None, &[],
        );
        assert!(result.is_some());
        let meta = result.unwrap();
        assert_eq!(meta.match_type, "exact");
        assert_eq!(meta.confidence, 1.0);
        assert_eq!(meta.line_idx, 0);
    }

    #[test]
    fn test_match_with_metadata_fuzzy() {
        let lines = vec![
            "The quick brown fox jumps over the lazy dog",
            "",
            "A completely different paragraph about something else",
        ];
        // A needle close to the first paragraph but not exact
        let result = find_match_line_scoped_with_metadata(
            "quick brown fox jumps over lazy dog", &lines, 0.5, None, &[],
        );
        assert!(result.is_some());
        let meta = result.unwrap();
        // Should be either exact or fuzzy (depends on normalization)
        assert!(
            meta.match_type == "exact" || meta.match_type == "fuzzy",
            "Expected exact or fuzzy, got: {}",
            meta.match_type
        );
        assert!(meta.confidence >= 0.5);
    }

    #[test]
    fn test_match_with_metadata_empty_needle() {
        let lines = vec!["some text"];
        let result = find_match_line_scoped_with_metadata(
            "", &lines, 0.65, None, &[],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_match_with_metadata_no_match() {
        let lines = vec!["The quick brown fox"];
        let result = find_match_line_scoped_with_metadata(
            "completely unrelated text about quantum physics", &lines, 0.9, None, &[],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_match_with_metadata_scoped_to_page() {
        let lines = vec![
            "<!-- Page 1 - 0 images -->",
            "First page content about biology",
            "",
            "<!-- Page 2 - 0 images -->",
            "Second page content about chemistry",
            "",
            "<!-- Page 3 - 0 images -->",
            "Third page content about physics",
        ];
        let page_ranges = find_page_ranges(&lines);
        let result = find_match_line_scoped_with_metadata(
            "content about chemistry", &lines, 0.65, Some("2"), &page_ranges,
        );
        assert!(result.is_some());
        let meta = result.unwrap();
        assert_eq!(meta.line_idx, 4);
    }
}
