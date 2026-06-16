use crate::zotero::types::{ZoteroAnnotation, ZoteroChildNote};

pub(crate) fn ann_type_name(ann_type: i32) -> &'static str {
    match ann_type {
        1 => "highlight",
        2 => "note",
        5 => "underline",
        6 => "freetext",
        _ => "annotation",
    }
}

pub fn truncate_anchor(text: &str, max_len: usize) -> String {
    let trimmed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.len() <= max_len {
        return trimmed;
    }
    // Find the last char boundary at or before max_len to avoid
    // panicking on multi-byte UTF-8 sequences.
    let boundary = trimmed
        .char_indices()
        .take_while(|&(i, _)| i <= max_len)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    let safe_slice = &trimmed[..boundary];
    match safe_slice.rfind(' ') {
        Some(pos) => format!("{}\u{2026}", &safe_slice[..pos]),
        None => format!("{}\u{2026}", safe_slice),
    }
}

pub fn escape_anchor(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Generate a Lit annotation DSL string for a single Zotero annotation.
pub fn zotero_ann_to_dsl(ann: &ZoteroAnnotation) -> String {
    let uuid = format!("zot-{}", ann.item_id);
    let type_name = ann_type_name(ann.ann_type);

    // Build page prefix
    let page_prefix = match ann.page_label.as_deref() {
        Some(p) if !p.is_empty() => format!("p. {} \u{2014} {}", p, type_name),
        _ => type_name.to_string(),
    };

    // Build body
    let body = match ann.comment.as_deref() {
        Some(c) if !c.is_empty() => format!("{}: {}", page_prefix, c),
        _ => page_prefix,
    };

    // Build scope anchor
    let anchor = ann
        .text
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(|t| truncate_anchor(t, 60));

    // Build compact form candidate
    let scope_part = match &anchor {
        Some(a) => format!(r#" ^"{}""#, escape_anchor(a)),
        None => String::new(),
    };

    let compact = format!("<!---[{}] n:{} | {} --->", uuid, scope_part, body);

    if compact.len() <= 120 {
        compact
    } else {
        let mut lines = vec![format!("<!---[{}]", uuid), "n:".to_string()];
        if let Some(a) = &anchor {
            lines.push(format!(r#"^"{}""#, escape_anchor(a)));
        }
        lines.push("---".to_string());
        lines.push(body);
        lines.push("--->".to_string());
        lines.join("\n")
    }
}

/// Generate a Lit annotation DSL string for a Zotero child note.
pub fn zotero_note_to_dsl(note: &ZoteroChildNote) -> String {
    let uuid = format!("zot-note-{}", note.item_id);
    let body = match note.title.as_deref() {
        Some(t) if !t.is_empty() => format!("{}: {}", t, note.html_content),
        _ => note.html_content.clone(),
    };

    let compact = format!("<!---[{}] n: | {} --->", uuid, body);

    if compact.len() <= 120 {
        compact
    } else {
        format!("<!---[{}]\nn:\n---\n{}\n--->", uuid, body)
    }
}

/// Format a collection of unmatched annotations into a markdown section.
pub fn collect_unmatched_section(unmatched: &[ZoteroAnnotation]) -> String {
    let mut parts = vec![
        "## Unmatched Zotero Annotations".to_string(),
        String::new(),
    ];
    for ann in unmatched {
        parts.push(zotero_ann_to_dsl(ann));
        parts.push(String::new());
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ann_type_name_known_types() {
        assert_eq!(ann_type_name(1), "highlight");
        assert_eq!(ann_type_name(2), "note");
        assert_eq!(ann_type_name(5), "underline");
        assert_eq!(ann_type_name(6), "freetext");
    }

    #[test]
    fn ann_type_name_unknown_type() {
        assert_eq!(ann_type_name(99), "annotation");
    }

    // -----------------------------------------------------------------------
    // truncate_anchor tests
    // -----------------------------------------------------------------------

    #[test]
    fn truncate_anchor_short_text() {
        assert_eq!(truncate_anchor("short text", 60), "short text");
    }

    #[test]
    fn truncate_anchor_exact_60() {
        let text = "a".repeat(60);
        assert_eq!(truncate_anchor(&text, 60), text);
    }

    #[test]
    fn truncate_anchor_long_text_breaks_at_space() {
        let text = "the quick brown fox jumps over the lazy dog and keeps running far away into the distance";
        let result = truncate_anchor(text, 60);
        assert!(result.ends_with('\u{2026}'));
        assert!(!result.contains("distance"));
        // The text before the ellipsis should be <= 60 chars
        let before_ellipsis = &result[..result.len() - '\u{2026}'.len_utf8()];
        assert!(before_ellipsis.len() <= 60);
    }

    #[test]
    fn truncate_anchor_normalizes_whitespace() {
        let text = "  hello   world  ";
        assert_eq!(truncate_anchor(text, 60), "hello world");
    }

    // -----------------------------------------------------------------------
    // escape_anchor tests
    // -----------------------------------------------------------------------

    #[test]
    fn escape_anchor_no_specials() {
        assert_eq!(escape_anchor("hello world"), "hello world");
    }

    #[test]
    fn escape_anchor_quotes() {
        assert_eq!(
            escape_anchor(r#"a "quoted" word"#),
            r#"a \"quoted\" word"#
        );
    }

    #[test]
    fn escape_anchor_backslash() {
        assert_eq!(escape_anchor(r"back\slash"), r"back\\slash");
    }

    // -----------------------------------------------------------------------
    // zotero_ann_to_dsl tests
    // -----------------------------------------------------------------------

    #[test]
    fn dsl_highlight_with_comment() {
        let ann = ZoteroAnnotation {
            item_id: 301,
            ann_type: 1,
            text: Some("highlighted text one".to_string()),
            comment: Some("my comment".to_string()),
            color: Some("#ffd400".to_string()),
            page_label: Some("5".to_string()),
            sort_index: "00005|000100|00000".to_string(),
        };
        let dsl = zotero_ann_to_dsl(&ann);
        assert!(dsl.contains("[zot-301]"));
        assert!(dsl.contains("n:"));
        assert!(dsl.contains(r#"^"highlighted text one""#));
        assert!(dsl.contains("p. 5"));
        assert!(dsl.contains("highlight"));
        assert!(dsl.contains("my comment"));
    }

    #[test]
    fn dsl_highlight_without_comment() {
        let ann = ZoteroAnnotation {
            item_id: 305,
            ann_type: 1,
            text: Some("highlighted text on page one".to_string()),
            comment: None,
            color: Some("#ffd400".to_string()),
            page_label: Some("1".to_string()),
            sort_index: "00001|000020|00000".to_string(),
        };
        let dsl = zotero_ann_to_dsl(&ann);
        assert!(dsl.contains("[zot-305]"));
        assert!(dsl.contains(r#"^"highlighted text on page one""#));
        assert!(dsl.contains("p. 1"));
        assert!(dsl.contains("highlight"));
        // Body should end with just the type name, no colon after it
        assert!(dsl.contains("highlight --->") || dsl.contains("highlight\n"));
    }

    #[test]
    fn dsl_sticky_note_no_text() {
        let ann = ZoteroAnnotation {
            item_id: 302,
            ann_type: 2,
            text: None,
            comment: Some("sticky note text".to_string()),
            color: Some("#ff6666".to_string()),
            page_label: Some("3".to_string()),
            sort_index: "00003|000050|00000".to_string(),
        };
        let dsl = zotero_ann_to_dsl(&ann);
        assert!(dsl.contains("[zot-302]"));
        assert!(dsl.contains("n:"));
        assert!(!dsl.contains(r#"^""#)); // no anchor scope
        assert!(dsl.contains("p. 3"));
        assert!(dsl.contains("note"));
        assert!(dsl.contains("sticky note text"));
    }

    #[test]
    fn dsl_long_text_uses_block_form() {
        let long_text = "This is a very long highlighted text that exceeds the normal threshold for compact annotations and should cause block form to be used instead of compact form";
        let ann = ZoteroAnnotation {
            item_id: 999,
            ann_type: 1,
            text: Some(long_text.to_string()),
            comment: Some("A very detailed comment about this highlight that makes the total length quite substantial".to_string()),
            color: None,
            page_label: Some("42".to_string()),
            sort_index: "00042|000100|00000".to_string(),
        };
        let dsl = zotero_ann_to_dsl(&ann);
        assert!(dsl.contains("<!---[zot-999]"));
        assert!(dsl.contains('\n')); // block form
        assert!(dsl.contains("--->"));
    }

    // -----------------------------------------------------------------------
    // zotero_note_to_dsl tests
    // -----------------------------------------------------------------------

    #[test]
    fn note_dsl_without_title() {
        let note = ZoteroChildNote {
            item_id: 400,
            html_content: "plain text content".to_string(),
            title: None,
        };
        let dsl = zotero_note_to_dsl(&note);
        assert!(dsl.contains("[zot-note-400]"));
        assert!(dsl.contains("n:"));
        assert!(dsl.contains("plain text content"));
    }

    #[test]
    fn note_dsl_with_title() {
        let note = ZoteroChildNote {
            item_id: 401,
            html_content: "note body".to_string(),
            title: Some("My Title".to_string()),
        };
        let dsl = zotero_note_to_dsl(&note);
        assert!(dsl.contains("[zot-note-401]"));
        assert!(dsl.contains("My Title: note body"));
    }

    // -----------------------------------------------------------------------
    // collect_unmatched_section tests
    // -----------------------------------------------------------------------

    #[test]
    fn unmatched_section_format() {
        let anns = vec![ZoteroAnnotation {
            item_id: 10,
            ann_type: 2,
            text: None,
            comment: Some("a note".to_string()),
            color: None,
            page_label: Some("1".to_string()),
            sort_index: "00001|000001|00000".to_string(),
        }];
        let section = collect_unmatched_section(&anns);
        assert!(section.starts_with("## Unmatched Zotero Annotations"));
        assert!(section.contains("[zot-10]"));
        assert!(section.contains("a note"));
    }

    #[test]
    fn unmatched_section_empty() {
        let section = collect_unmatched_section(&[]);
        assert!(section.starts_with("## Unmatched Zotero Annotations"));
    }

    // -----------------------------------------------------------------------
    // truncate_anchor multibyte UTF-8
    // -----------------------------------------------------------------------

    #[test]
    fn test_truncate_anchor_multibyte_accented() {
        // Each accented char is 2 bytes in UTF-8.
        // "cafe\u{0301}" normalizes to 5 chars but the e-acute could be 2 bytes.
        // Use a string that would panic with naive byte slicing.
        let text = "\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}"; // "eeeee" with accents, 10 bytes, 5 chars
        // max_len=3 is in the middle of multi-byte territory
        let result = truncate_anchor(text, 3);
        // Should not panic, and should produce a valid string with ellipsis
        assert!(result.ends_with('\u{2026}'));
        // The part before the ellipsis should be valid UTF-8 (it compiles and runs = valid)
        assert!(!result.is_empty());
    }

    #[test]
    fn test_truncate_anchor_cjk() {
        // Each CJK char is 3 bytes. "hello" is 5 bytes.
        // "\u{4e16}\u{754c}" = "世界" = 6 bytes, 2 chars
        let text = "hello \u{4e16}\u{754c} world more words to exceed the limit and test truncation behavior with CJK";
        let result = truncate_anchor(text, 10);
        assert!(result.ends_with('\u{2026}'));
        let before_ellipsis = &result[..result.len() - '\u{2026}'.len_utf8()];
        // Should break at a word boundary
        assert!(
            before_ellipsis.ends_with("hello"),
            "expected word break: got '{}'",
            before_ellipsis
        );
    }

    #[test]
    fn test_truncate_anchor_emoji() {
        // Emoji can be 4 bytes each
        let text = "\u{1f600}\u{1f600}\u{1f600} hello world";
        let result = truncate_anchor(text, 5);
        // Should not panic
        assert!(result.ends_with('\u{2026}'));
    }
}
