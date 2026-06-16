/// Collapse all whitespace runs to a single space, trim, and lowercase.
pub fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// OCR-aware normalization. On top of basic `normalize()` (whitespace collapse + lowercase):
/// - Rejoin end-of-line hyphenation: "knowl-\nedge" -> "knowledge"
/// - Normalize Unicode ligatures: fi, fl, ffi, ffl, st
/// - Normalize confusable punctuation: curly quotes -> straight, em/en dash -> hyphen
pub fn ocr_normalize(text: &str) -> String {
    let mut s = text.to_string();

    // 1. Rejoin end-of-line hyphenation BEFORE whitespace collapse.
    //    Pattern: hyphen-minus at end of a word, followed by newline and optional
    //    whitespace, then a lowercase letter continuing the word.
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' {
            // Look ahead: skip optional \r, require \n, skip whitespace,
            // check next char is lowercase alphabetic
            let mut j = i + 1;
            if j < chars.len() && chars[j] == '\r' {
                j += 1;
            }
            if j < chars.len() && chars[j] == '\n' {
                j += 1;
                while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_lowercase() {
                    // Rejoin: skip the hyphen and whitespace
                    i = j;
                    continue;
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    s = result;

    // 2. Unicode ligature normalization
    s = s.replace('\u{FB00}', "ff"); // ff ligature
    s = s.replace('\u{FB01}', "fi"); // fi ligature
    s = s.replace('\u{FB02}', "fl"); // fl ligature
    s = s.replace('\u{FB03}', "ffi"); // ffi ligature
    s = s.replace('\u{FB04}', "ffl"); // ffl ligature
    s = s.replace('\u{FB06}', "st"); // st ligature

    // 3. Confusable punctuation
    // Curly/smart quotes -> straight
    s = s.replace('\u{2018}', "'"); // left single curly quote
    s = s.replace('\u{2019}', "'"); // right single curly quote / apostrophe
    s = s.replace('\u{201C}', "\""); // left double curly quote
    s = s.replace('\u{201D}', "\""); // right double curly quote
    // Dashes -> hyphen-minus
    s = s.replace('\u{2013}', "-"); // en dash
    s = s.replace('\u{2014}', "-"); // em dash
    s = s.replace('\u{2212}', "-"); // minus sign

    // 4. Apply standard normalize (whitespace collapse + lowercase)
    normalize(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_collapses_whitespace() {
        assert_eq!(normalize("  hello   world  "), "hello world");
    }

    #[test]
    fn test_normalize_lowercases() {
        assert_eq!(normalize("Hello World"), "hello world");
    }

    #[test]
    fn test_normalize_handles_newlines() {
        assert_eq!(normalize("line1\nline2\tline3"), "line1 line2 line3");
    }

    #[test]
    fn test_normalize_empty_string() {
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn test_normalize_whitespace_only() {
        assert_eq!(normalize("   \t\n  "), "");
    }

    // -------------------------------------------------------------------
    // ocr_normalize tests
    // -------------------------------------------------------------------

    #[test]
    fn test_ocr_normalize_hyphenation() {
        assert_eq!(ocr_normalize("knowl-\nedge base"), "knowledge base");
    }

    #[test]
    fn test_ocr_normalize_hyphenation_with_spaces() {
        assert_eq!(ocr_normalize("knowl-\n  edge"), "knowledge");
    }

    #[test]
    fn test_ocr_normalize_hyphenation_not_joined_uppercase() {
        // "Smith-\nJones" should NOT be joined because J is uppercase
        let result = ocr_normalize("Smith-\nJones");
        assert!(
            result.contains("-"),
            "uppercase after hyphen should not be joined: '{}'",
            result
        );
    }

    #[test]
    fn test_ocr_normalize_hyphenation_crlf() {
        assert_eq!(ocr_normalize("knowl-\r\nedge"), "knowledge");
    }

    #[test]
    fn test_ocr_normalize_ligatures() {
        assert_eq!(ocr_normalize("\u{FB01}rst \u{FB02}oor"), "first floor");
    }

    #[test]
    fn test_ocr_normalize_ff_ligatures() {
        assert_eq!(ocr_normalize("\u{FB00}ect"), "ffect");
        assert_eq!(ocr_normalize("\u{FB03}ce"), "ffice");
        assert_eq!(ocr_normalize("\u{FB04}e"), "ffle");
    }

    #[test]
    fn test_ocr_normalize_st_ligature() {
        assert_eq!(ocr_normalize("\u{FB06}yle"), "style");
    }

    #[test]
    fn test_ocr_normalize_curly_quotes() {
        assert_eq!(
            ocr_normalize("\u{201C}hello\u{201D}"),
            "\"hello\""
        );
        assert_eq!(
            ocr_normalize("\u{2018}it\u{2019}s"),
            "'it's"
        );
    }

    #[test]
    fn test_ocr_normalize_dashes() {
        assert_eq!(ocr_normalize("a\u{2014}b\u{2013}c"), "a-b-c");
        assert_eq!(ocr_normalize("x\u{2212}y"), "x-y"); // minus sign
    }

    #[test]
    fn test_ocr_normalize_combined() {
        assert_eq!(
            ocr_normalize("The \u{FB01}eld of knowl-\nedge\u{2014}based systems"),
            "the field of knowledge-based systems"
        );
    }

    #[test]
    fn test_ocr_normalize_plain_text() {
        assert_eq!(ocr_normalize("hello world"), "hello world");
    }

    #[test]
    fn test_ocr_normalize_empty() {
        assert_eq!(ocr_normalize(""), "");
    }
}
