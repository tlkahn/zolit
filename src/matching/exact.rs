use crate::matching::normalize::normalize;

/// Search for `needle` as an exact normalized substring across all lines joined.
/// Returns the 0-based line index where the match ENDS (insertion point).
/// Returns `None` if the needle is not found.
pub fn find_exact_match(needle: &str, lines: &[&str]) -> Option<usize> {
    let mut joined = String::new();
    let mut line_starts: Vec<(usize, usize)> = Vec::with_capacity(lines.len());

    let mut pos: usize = 0;
    for (i, line) in lines.iter().enumerate() {
        if !joined.is_empty() {
            joined.push(' ');
            pos += 1;
        }
        line_starts.push((pos, i));
        let nl = normalize(line);
        joined.push_str(&nl);
        pos += nl.len();
    }

    let norm_needle = normalize(needle);
    let idx = joined.find(&norm_needle)?;
    let end = idx + norm_needle.len();

    let mut result = 0;
    for &(start, li) in &line_starts {
        if start <= end {
            result = li;
        } else {
            break;
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match_single_line() {
        let lines = vec!["The quick brown fox jumps over the lazy dog"];
        assert_eq!(find_exact_match("brown fox", &lines), Some(0));
    }

    #[test]
    fn test_exact_match_spans_lines() {
        let lines = vec!["end of first", "start of second"];
        assert_eq!(find_exact_match("first start", &lines), Some(1));
    }

    #[test]
    fn test_exact_match_returns_end_line() {
        let lines = vec!["aaa bbb", "ccc ddd", "eee fff"];
        // "bbb ccc ddd eee" spans lines 0-2, ends in line 2
        assert_eq!(find_exact_match("bbb ccc ddd eee", &lines), Some(2));
    }

    #[test]
    fn test_exact_match_not_found() {
        let lines = vec!["hello world"];
        assert_eq!(find_exact_match("goodbye", &lines), None);
    }

    #[test]
    fn test_exact_match_whitespace_normalized() {
        let lines = vec!["word   one    two"];
        assert_eq!(find_exact_match("one  two", &lines), Some(0));
    }

    #[test]
    fn test_exact_match_case_insensitive() {
        let lines = vec!["The Quick Brown Fox"];
        assert_eq!(find_exact_match("quick brown", &lines), Some(0));
    }
}
