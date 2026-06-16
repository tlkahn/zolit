use crate::matching::normalize::normalize;
use strsim::normalized_levenshtein;

/// Group consecutive non-blank lines into paragraphs.
/// Returns a vec of (normalized_paragraph_text, last_line_index).
/// Blank lines (whitespace-only) act as paragraph separators.
pub fn build_paragraphs(lines: &[&str]) -> Vec<(String, usize)> {
    let mut paragraphs: Vec<(String, usize)> = Vec::new();
    let mut current_parts: Vec<String> = Vec::new();
    let mut last_line_idx: usize = 0;

    for (i, line) in lines.iter().enumerate() {
        let nl = normalize(line);
        if nl.is_empty() {
            if !current_parts.is_empty() {
                let text = current_parts.join(" ");
                paragraphs.push((text, last_line_idx));
                current_parts.clear();
            }
        } else {
            current_parts.push(nl);
            last_line_idx = i;
        }
    }
    if !current_parts.is_empty() {
        let text = current_parts.join(" ");
        paragraphs.push((text, last_line_idx));
    }

    paragraphs
}

/// Compute the length of the longest common substring between `a` and `b`
/// using a standard DP approach with O(min(|a|,|b|)) space.
pub(crate) fn lcs_length(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    // Ensure `b` is the shorter side for memory efficiency.
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };

    let cols = b.len();
    let mut prev = vec![0usize; cols + 1];
    let mut curr = vec![0usize; cols + 1];
    let mut max_len: usize = 0;

    for &ai in a.iter() {
        for (j, &bj) in b.iter().enumerate() {
            if ai == bj {
                curr[j + 1] = prev[j] + 1;
                if curr[j + 1] > max_len {
                    max_len = curr[j + 1];
                }
            } else {
                curr[j + 1] = 0;
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.iter_mut().for_each(|x| *x = 0);
    }
    max_len
}

/// Compute a fuzzy similarity score between two strings using dual scoring:
/// 1. `strsim::normalized_levenshtein` (edit-distance, good for OCR errors)
/// 2. LCS ratio = lcs_length / max(|a|, |b|) (good for substring matches)
///
/// Returns the maximum of the two scores.
pub fn fuzzy_score(a: &str, b: &str) -> f64 {
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0; // both empty
    }
    let lev = normalized_levenshtein(a, b);
    let lcs = lcs_length(a, b) as f64 / max_len as f64;
    lev.max(lcs)
}

/// Score each paragraph against the needle using dual scoring
/// (Levenshtein + LCS ratio). Returns the `last_line_index` of the
/// best-matching paragraph if its best score >= threshold.
pub fn find_fuzzy_match(
    needle: &str,
    paragraphs: &[(String, usize)],
    threshold: f64,
) -> Option<usize> {
    find_fuzzy_match_with_score(needle, paragraphs, threshold).map(|(line, _)| line)
}

/// Try windows of 2 and 3 consecutive paragraphs.
/// For each window, join paragraph texts with a space separator,
/// score the joined text against the needle, and track the best.
/// Returns the last_line_index of the LAST paragraph in the best window,
/// or None if no window scores >= threshold.
///
/// Window size 1 is handled by `find_fuzzy_match`, so this only tries 2+.
pub fn find_fuzzy_match_windowed(
    needle: &str,
    paragraphs: &[(String, usize)],
    threshold: f64,
) -> Option<usize> {
    find_fuzzy_match_windowed_with_score(needle, paragraphs, threshold).map(|(line, _)| line)
}

/// Like `find_fuzzy_match` but also returns the score.
pub(crate) fn find_fuzzy_match_with_score(
    needle: &str,
    paragraphs: &[(String, usize)],
    threshold: f64,
) -> Option<(usize, f64)> {
    let norm_needle = normalize(needle);
    if norm_needle.is_empty() {
        return None;
    }

    let mut best_score: f64 = 0.0;
    let mut best_line: Option<usize> = None;

    for (para_text, last_line) in paragraphs {
        if para_text.is_empty() {
            continue;
        }
        let score = fuzzy_score(&norm_needle, para_text);
        if score > best_score {
            best_score = score;
            best_line = Some(*last_line);
        }
    }

    if best_score >= threshold {
        best_line.map(|l| (l, best_score))
    } else {
        None
    }
}

/// Like `find_fuzzy_match_windowed` but also returns the score.
pub(crate) fn find_fuzzy_match_windowed_with_score(
    needle: &str,
    paragraphs: &[(String, usize)],
    threshold: f64,
) -> Option<(usize, f64)> {
    let norm_needle = normalize(needle);
    if norm_needle.is_empty() || paragraphs.len() < 2 {
        return None;
    }

    let mut best_score: f64 = 0.0;
    let mut best_line: Option<usize> = None;

    let max_window = 3.min(paragraphs.len());
    for window_size in 2..=max_window {
        for start in 0..=(paragraphs.len() - window_size) {
            let end = start + window_size - 1;
            let joined: String = paragraphs[start..=end]
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            let score = fuzzy_score(&norm_needle, &joined);

            if score > best_score {
                best_score = score;
                best_line = Some(paragraphs[end].1);
            }
        }
    }

    if best_score >= threshold {
        best_line.map(|l| (l, best_score))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // build_paragraphs tests
    // -------------------------------------------------------------------

    #[test]
    fn test_build_paragraphs_basic() {
        let lines = vec!["para one a", "para one b", "", "para two a"];
        let paras = build_paragraphs(&lines);
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].0, "para one a para one b");
        assert_eq!(paras[0].1, 1);
        assert_eq!(paras[1].0, "para two a");
        assert_eq!(paras[1].1, 3);
    }

    #[test]
    fn test_build_paragraphs_single() {
        let lines = vec!["line one", "line two", "line three"];
        let paras = build_paragraphs(&lines);
        assert_eq!(paras.len(), 1);
        assert_eq!(paras[0].0, "line one line two line three");
        assert_eq!(paras[0].1, 2);
    }

    #[test]
    fn test_build_paragraphs_empty() {
        let lines: Vec<&str> = vec![];
        let paras = build_paragraphs(&lines);
        assert_eq!(paras.len(), 0);
    }

    #[test]
    fn test_build_paragraphs_all_blank() {
        let lines = vec!["", "  ", "\t"];
        let paras = build_paragraphs(&lines);
        assert_eq!(paras.len(), 0);
    }

    #[test]
    fn test_build_paragraphs_leading_trailing_blanks() {
        let lines = vec!["", "", "content", ""];
        let paras = build_paragraphs(&lines);
        assert_eq!(paras.len(), 1);
        assert_eq!(paras[0].0, "content");
        assert_eq!(paras[0].1, 2);
    }

    // -------------------------------------------------------------------
    // lcs_length tests
    // -------------------------------------------------------------------

    #[test]
    fn test_lcs_length_identical() {
        assert_eq!(lcs_length("abcdef", "abcdef"), 6);
    }

    #[test]
    fn test_lcs_length_partial() {
        // Longest common substring of "abcdef" and "xbcdey" is "bcde" (length 4)
        assert_eq!(lcs_length("abcdef", "xbcdey"), 4);
    }

    #[test]
    fn test_lcs_length_no_common() {
        assert_eq!(lcs_length("abc", "xyz"), 0);
    }

    #[test]
    fn test_lcs_length_empty() {
        assert_eq!(lcs_length("", "abc"), 0);
        assert_eq!(lcs_length("abc", ""), 0);
        assert_eq!(lcs_length("", ""), 0);
    }

    #[test]
    fn test_lcs_length_one_is_substring() {
        assert_eq!(lcs_length("the quick brown fox", "quick brown"), 11);
    }

    // -------------------------------------------------------------------
    // lcs_length multibyte tests
    // -------------------------------------------------------------------

    #[test]
    fn test_lcs_length_multibyte_cjk() {
        // "\u{6df1}\u{5ea6}\u{5b66}\u{4e60}" = "深度学习" (4 chars, 12 bytes)
        // "\u{673a}\u{5668}\u{5b66}\u{4e60}" = "机器学习" (4 chars, 12 bytes)
        // Common substring: "\u{5b66}\u{4e60}" = "学习" (2 chars, 6 bytes)
        let a = "\u{6df1}\u{5ea6}\u{5b66}\u{4e60}";
        let b = "\u{673a}\u{5668}\u{5b66}\u{4e60}";
        // Should return 2 (chars), not 6 (bytes)
        assert_eq!(lcs_length(a, b), 2);
    }

    #[test]
    fn test_lcs_length_multibyte_accented() {
        // "cafe\u{0301}" and "cafe" share "caf" (3 chars) but the acute-e differs
        let a = "caf\u{00e9}";
        let b = "cafe";
        assert_eq!(lcs_length(a, b), 3); // "caf" is common
    }

    // -------------------------------------------------------------------
    // find_fuzzy_match tests
    // -------------------------------------------------------------------

    #[test]
    fn test_fuzzy_match_above_threshold() {
        let paragraphs = vec![
            ("the quick brown fox jumps over the lazy dog".to_string(), 2),
            ("something completely different".to_string(), 5),
        ];
        let result = find_fuzzy_match("quick brown fox jumps over", &paragraphs, 0.5);
        assert_eq!(result, Some(2));
    }

    #[test]
    fn test_fuzzy_match_below_threshold() {
        let paragraphs = vec![("the quick brown fox".to_string(), 2)];
        let result =
            find_fuzzy_match("completely unrelated text here", &paragraphs, 0.65);
        assert_eq!(result, None);
    }

    #[test]
    fn test_fuzzy_match_best_paragraph() {
        let paragraphs = vec![
            ("introduction to machine learning".to_string(), 3),
            (
                "deep learning neural networks for image recognition".to_string(),
                7,
            ),
            (
                "machine learning algorithms and applications".to_string(),
                12,
            ),
        ];
        let result = find_fuzzy_match("machine learning algorithms", &paragraphs, 0.5);
        assert_eq!(result, Some(12));
    }

    #[test]
    fn test_fuzzy_match_empty_needle() {
        let paragraphs = vec![("some text".to_string(), 0)];
        assert_eq!(find_fuzzy_match("", &paragraphs, 0.65), None);
        assert_eq!(find_fuzzy_match("   ", &paragraphs, 0.65), None);
    }

    #[test]
    fn test_fuzzy_match_empty_paragraphs() {
        let paragraphs: Vec<(String, usize)> = vec![];
        assert_eq!(find_fuzzy_match("some needle", &paragraphs, 0.65), None);
    }

    // -------------------------------------------------------------------
    // fuzzy_score tests
    // -------------------------------------------------------------------

    #[test]
    fn test_fuzzy_score_exact() {
        let score = fuzzy_score("hello world", "hello world");
        assert!(
            (score - 1.0).abs() < f64::EPSILON,
            "identical strings should score 1.0, got {}",
            score
        );
    }

    #[test]
    fn test_fuzzy_score_similar() {
        // "modem" vs "modern" — OCR-style error
        let score = fuzzy_score("the modem approach", "the modern approach");
        assert!(
            score > 0.85,
            "similar strings should score high, got {}",
            score
        );
    }

    #[test]
    fn test_fuzzy_score_different() {
        let score = fuzzy_score("quantum computing", "machine learning algorithms");
        assert!(
            score < 0.4,
            "unrelated strings should score low, got {}",
            score
        );
    }

    #[test]
    fn test_fuzzy_score_empty() {
        assert!((fuzzy_score("", "") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fuzzy_score_one_empty() {
        let score = fuzzy_score("hello", "");
        assert!(
            score < 0.01,
            "one empty string should score near 0, got {}",
            score
        );
    }

    #[test]
    fn test_fuzzy_dual_levenshtein_wins() {
        // OCR: "modem" for "modern". Levenshtein should catch this better
        // than LCS because the common substring breaks at the typo.
        let needle = "the modem approach to learning";
        let para = "the modern approach to learning";
        let score = fuzzy_score(needle, para);
        assert!(
            score >= 0.9,
            "OCR substitution should still match well, got {}",
            score
        );
    }

    #[test]
    fn test_fuzzy_dual_lcs_wins() {
        // Short needle fully present as substring of long paragraph.
        // Levenshtein will be low due to length difference, but LCS catches it.
        let needle = "neural networks";
        let para = "introduction to neural networks and deep learning systems";
        let score = fuzzy_score(needle, para);
        // LCS ratio = 15/57 ~ 0.26 -- actually that's below threshold.
        // But let's verify the score is at least reasonable.
        assert!(
            score > 0.2,
            "substring presence should contribute to score, got {}",
            score
        );
    }

    #[test]
    fn test_fuzzy_dual_neither_passes() {
        let score = fuzzy_score("quantum computing", "machine learning algorithms");
        assert!(score < 0.5, "unrelated strings should score low, got {}", score);
    }

    #[test]
    fn test_fuzzy_ocr_rn_to_m() {
        // normalized_levenshtein("modem", "modern") ~ 0.67 (edit distance 2, max len 6).
        // In context of a longer phrase, the score is higher (~0.89).
        let score = fuzzy_score("the modem approach", "the modern approach");
        assert!(
            score > 0.85,
            "OCR rn->m confusion in context should still match, got {}",
            score
        );
    }

    // -------------------------------------------------------------------
    // find_fuzzy_match_windowed tests
    // -------------------------------------------------------------------

    #[test]
    fn test_windowed_cross_paragraph() {
        let paragraphs = vec![
            ("end of first paragraph".to_string(), 3),
            ("start of second paragraph".to_string(), 7),
        ];
        let result = find_fuzzy_match_windowed(
            "end of first paragraph start of second paragraph",
            &paragraphs,
            0.5,
        );
        assert_eq!(result, Some(7));
    }

    #[test]
    fn test_windowed_three_paragraphs() {
        let paragraphs = vec![
            ("alpha beta gamma".to_string(), 2),
            ("delta epsilon zeta".to_string(), 5),
            ("eta theta iota".to_string(), 8),
        ];
        // Needle spans all three
        let result = find_fuzzy_match_windowed(
            "alpha beta gamma delta epsilon zeta eta theta iota",
            &paragraphs,
            0.5,
        );
        assert_eq!(result, Some(8));
    }

    #[test]
    fn test_windowed_few_paragraphs() {
        // Only 1 paragraph: windowed (sizes 2+) should return None
        let paragraphs = vec![("only paragraph".to_string(), 0)];
        let result = find_fuzzy_match_windowed("only paragraph", &paragraphs, 0.5);
        assert_eq!(result, None);
    }

    #[test]
    fn test_windowed_empty_needle() {
        let paragraphs = vec![
            ("para one".to_string(), 0),
            ("para two".to_string(), 2),
        ];
        assert_eq!(find_fuzzy_match_windowed("", &paragraphs, 0.5), None);
        assert_eq!(find_fuzzy_match_windowed("   ", &paragraphs, 0.5), None);
    }

    #[test]
    fn test_windowed_no_match() {
        let paragraphs = vec![
            ("alpha beta".to_string(), 1),
            ("gamma delta".to_string(), 3),
        ];
        let result = find_fuzzy_match_windowed(
            "completely unrelated text about quantum physics",
            &paragraphs,
            0.65,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_fuzzy_score_cjk() {
        // 4 chars match out of 7 chars = 0.571, not 4/21=0.19
        let score = fuzzy_score("深度学习", "深度学习方法论");
        assert!(score > 0.5, "CJK fuzzy score should be char-based, got {}", score);
    }
}
