use crate::matching::fuzzy::build_paragraphs;

/// A page range: the line indices (inclusive) that belong to a given OCR page.
/// `page_index` is the zero-based OCR page number from the `<!-- Page N -->` comment.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRange {
    pub page_index: usize,
    pub start_line: usize,
    pub end_line: usize, // inclusive
}

/// Parse `<!-- Page N - M images -->` comments from OCR markdown lines.
/// Returns a sorted vec of PageRange. Lines between markers belong to the
/// preceding page. Lines before the first marker have no page assignment.
pub fn find_page_ranges(lines: &[&str]) -> Vec<PageRange> {
    let mut ranges = Vec::new();
    let mut current_page: Option<(usize, usize)> = None; // (page_index, start_line)

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("<!-- Page ") && trimmed.contains(" images -->") {
            // Close previous page range
            if let Some((prev_idx, prev_start)) = current_page {
                ranges.push(PageRange {
                    page_index: prev_idx,
                    start_line: prev_start,
                    end_line: i.saturating_sub(1),
                });
            }

            // Parse page number: extract N from "<!-- Page N - M images -->"
            let after_prefix = &trimmed["<!-- Page ".len()..];
            if let Some(dash_pos) = after_prefix.find(" -") {
                if let Ok(page_num) = after_prefix[..dash_pos].trim().parse::<usize>() {
                    current_page = Some((page_num, i));
                }
            }
        }
    }

    // Close final page range
    if let Some((prev_idx, prev_start)) = current_page {
        ranges.push(PageRange {
            page_index: prev_idx,
            start_line: prev_start,
            end_line: lines.len().saturating_sub(1),
        });
    }

    ranges
}

/// Given a Zotero page_label (typically "1", "2", etc. -- 1-based human page number),
/// and page ranges parsed from OCR markdown, return the line range (start, end inclusive)
/// covering the target page +/- 1 page margin. Returns None if page_label is not numeric
/// or no matching page ranges exist.
pub fn page_scoped_line_range(
    page_label: &str,
    page_ranges: &[PageRange],
) -> Option<(usize, usize)> {
    let human_page: usize = page_label.parse().ok()?;
    // Zotero page_label is 1-based, OCR page_index is 0-based
    let target_ocr_page = human_page.checked_sub(1)?;

    // Find ranges for target-1, target, target+1
    let min_page = target_ocr_page.saturating_sub(1);
    let max_page = target_ocr_page + 1;

    let matching: Vec<&PageRange> = page_ranges
        .iter()
        .filter(|r| r.page_index >= min_page && r.page_index <= max_page)
        .collect();

    if matching.is_empty() {
        return None;
    }

    let start = matching.iter().map(|r| r.start_line).min().unwrap();
    let end = matching.iter().map(|r| r.end_line).max().unwrap();
    Some((start, end))
}

/// Rejoin hyphenated word breaks that span line boundaries in paragraph text.
/// After lines are joined with spaces, patterns like "knowl- edge" (where a
/// word ends with hyphen-space and the next token starts lowercase) are
/// collapsed to "knowledge".
pub(crate) fn rejoin_paragraph_hyphens(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' {
            // Check: hyphen followed by space, then lowercase ASCII letter
            if i + 2 < chars.len()
                && chars[i + 1] == ' '
                && chars[i + 2].is_ascii_lowercase()
            {
                // Skip the hyphen and space, continue with the lowercase letter
                i += 2;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

/// Like `build_paragraphs` but applies `rejoin_paragraph_hyphens` to each
/// paragraph's joined text, handling line-break hyphenation across lines.
pub(crate) fn build_paragraphs_ocr(lines: &[&str]) -> Vec<(String, usize)> {
    let raw = build_paragraphs(lines);
    raw.into_iter()
        .map(|(text, last_line)| (rejoin_paragraph_hyphens(&text), last_line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // find_page_ranges tests
    // -------------------------------------------------------------------

    #[test]
    fn test_find_page_ranges_basic() {
        let lines = vec![
            "<!-- Page 0 - 2 images -->",
            "line 1",
            "line 2",
            "<!-- Page 1 - 0 images -->",
            "line 4",
            "line 5",
        ];
        let ranges = find_page_ranges(&lines);
        assert_eq!(ranges.len(), 2);
        assert_eq!(
            ranges[0],
            PageRange { page_index: 0, start_line: 0, end_line: 2 }
        );
        assert_eq!(
            ranges[1],
            PageRange { page_index: 1, start_line: 3, end_line: 5 }
        );
    }

    #[test]
    fn test_find_page_ranges_no_markers() {
        let lines = vec!["just text", "more text"];
        let ranges = find_page_ranges(&lines);
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_find_page_ranges_single_page() {
        let lines = vec!["<!-- Page 0 - 1 images -->", "content"];
        let ranges = find_page_ranges(&lines);
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges[0],
            PageRange { page_index: 0, start_line: 0, end_line: 1 }
        );
    }

    #[test]
    fn test_find_page_ranges_with_frontmatter() {
        let lines = vec![
            "---",
            "title: X",
            "---",
            "<!-- Page 0 - 0 images -->",
            "text",
        ];
        let ranges = find_page_ranges(&lines);
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges[0],
            PageRange { page_index: 0, start_line: 3, end_line: 4 }
        );
    }

    #[test]
    fn test_find_page_ranges_many_pages() {
        let lines = vec![
            "<!-- Page 0 - 1 images -->",
            "page 0 content",
            "<!-- Page 1 - 2 images -->",
            "page 1 content",
            "more page 1",
            "<!-- Page 2 - 0 images -->",
            "page 2 content",
            "<!-- Page 3 - 1 images -->",
            "page 3 content",
            "last line",
        ];
        let ranges = find_page_ranges(&lines);
        assert_eq!(ranges.len(), 4);
        assert_eq!(ranges[0].page_index, 0);
        assert_eq!(ranges[1].page_index, 1);
        assert_eq!(ranges[2].page_index, 2);
        assert_eq!(ranges[3].page_index, 3);
        assert_eq!(ranges[3].end_line, 9);
    }

    // -------------------------------------------------------------------
    // page_scoped_line_range tests
    // -------------------------------------------------------------------

    #[test]
    fn test_page_scoped_range_middle_page() {
        // Pages 0-4; page_label "3" means OCR page 2, scope = pages 1,2,3
        let ranges = vec![
            PageRange { page_index: 0, start_line: 0, end_line: 9 },
            PageRange { page_index: 1, start_line: 10, end_line: 19 },
            PageRange { page_index: 2, start_line: 20, end_line: 29 },
            PageRange { page_index: 3, start_line: 30, end_line: 39 },
            PageRange { page_index: 4, start_line: 40, end_line: 49 },
        ];
        let result = page_scoped_line_range("3", &ranges);
        // target_ocr_page = 2, scope = pages 1..3
        assert_eq!(result, Some((10, 39)));
    }

    #[test]
    fn test_page_scoped_range_first_page() {
        let ranges = vec![
            PageRange { page_index: 0, start_line: 0, end_line: 9 },
            PageRange { page_index: 1, start_line: 10, end_line: 19 },
            PageRange { page_index: 2, start_line: 20, end_line: 29 },
        ];
        // page_label "1" -> OCR page 0, scope = pages 0..1
        let result = page_scoped_line_range("1", &ranges);
        assert_eq!(result, Some((0, 19)));
    }

    #[test]
    fn test_page_scoped_range_last_page() {
        let ranges = vec![
            PageRange { page_index: 0, start_line: 0, end_line: 9 },
            PageRange { page_index: 1, start_line: 10, end_line: 19 },
            PageRange { page_index: 2, start_line: 20, end_line: 29 },
        ];
        // page_label "3" -> OCR page 2, scope = pages 1..3 (only 1,2 exist)
        let result = page_scoped_line_range("3", &ranges);
        assert_eq!(result, Some((10, 29)));
    }

    #[test]
    fn test_page_scoped_range_non_numeric_label() {
        let ranges = vec![
            PageRange { page_index: 0, start_line: 0, end_line: 9 },
        ];
        assert_eq!(page_scoped_line_range("iv", &ranges), None);
    }

    #[test]
    fn test_page_scoped_range_no_matching_pages() {
        let ranges = vec![
            PageRange { page_index: 0, start_line: 0, end_line: 9 },
            PageRange { page_index: 1, start_line: 10, end_line: 19 },
        ];
        // page_label "99" -> OCR page 98, scope = pages 97..99, none exist
        assert_eq!(page_scoped_line_range("99", &ranges), None);
    }

    #[test]
    fn test_page_scoped_range_page_zero_label() {
        // page_label "0" -> human_page 0 -> checked_sub(1) fails -> None
        let ranges = vec![
            PageRange { page_index: 0, start_line: 0, end_line: 9 },
        ];
        assert_eq!(page_scoped_line_range("0", &ranges), None);
    }

    #[test]
    fn test_page_scoped_range_empty_ranges() {
        assert_eq!(page_scoped_line_range("1", &[]), None);
    }

    #[test]
    fn test_rejoin_paragraph_hyphens_preserves_non_ascii() {
        assert_eq!(rejoin_paragraph_hyphens("über- schrift"), "überschrift");
        assert_eq!(rejoin_paragraph_hyphens("café"), "café");
        assert_eq!(rejoin_paragraph_hyphens("日本語テキスト"), "日本語テキスト");
    }
}
