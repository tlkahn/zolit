use std::collections::HashSet;

/// Extract all existing Zotero annotation IDs from markdown content.
///
/// Scans for `<!---[zot-...]` patterns and returns the set of IDs found
/// (e.g. `zot-301`, `zot-note-400`).
pub fn existing_zotero_ids(content: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut search_from = 0;
    while let Some(start) = content[search_from..].find("<!---[zot-") {
        let abs_start = search_from + start + 6; // skip "<!---["
        if let Some(end) = content[abs_start..].find(']') {
            let id = &content[abs_start..abs_start + end];
            if !id.is_empty() {
                ids.insert(id.to_string());
            }
            search_from = abs_start + end;
        } else {
            break;
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_ids_empty_content() {
        let ids = existing_zotero_ids("");
        assert!(ids.is_empty());
    }

    #[test]
    fn existing_ids_no_zotero_annotations() {
        let content = "# Title\n\nSome text\n\n<!---[abc-123] n: | a note --->";
        let ids = existing_zotero_ids(content);
        assert!(ids.is_empty());
    }

    #[test]
    fn existing_ids_finds_annotation_ids() {
        let content = r#"# Title

<!---[zot-301] n: ^"text" | comment --->

Some text

<!---[zot-note-400] n: | note content --->
"#;
        let ids = existing_zotero_ids(content);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("zot-301"));
        assert!(ids.contains("zot-note-400"));
    }

    #[test]
    fn existing_ids_handles_block_form() {
        let content = "<!---[zot-999]\nn:\n---\nbody\n--->";
        let ids = existing_zotero_ids(content);
        assert!(ids.contains("zot-999"));
    }

    #[test]
    fn existing_ids_ignores_non_zotero() {
        let content = "<!---[abc-123] n: | body --->\n<!---[zot-42] n: | body --->";
        let ids = existing_zotero_ids(content);
        assert_eq!(ids.len(), 1);
        assert!(ids.contains("zot-42"));
    }
}
