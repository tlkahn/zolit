/// Detect the line index of YAML frontmatter closing delimiter.
///
/// Returns `Some(line_index)` of the closing `---` if the file starts
/// with a `---` line. Returns `None` if there is no frontmatter.
pub(crate) fn detect_frontmatter_end(lines: &[String]) -> Option<usize> {
    if lines.first().map(|l| l.trim()) != Some("---") {
        return None;
    }
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            return Some(i);
        }
    }
    None
}

/// Insert annotation DSL strings into markdown content at specified line positions.
///
/// Each entry in `annotations_with_positions` is `(line_index, dsl_string)`.
/// Insertions happen after the target line. YAML frontmatter is respected:
/// insertions targeting lines inside frontmatter are clamped to after it.
pub fn insert_annotations_into_markdown(
    content: &str,
    annotations_with_positions: Vec<(usize, String)>,
) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let trailing_newline = content.ends_with('\n');

    let frontmatter_end = detect_frontmatter_end(&lines);

    // Tag each entry with its original order so we can break ties.
    let mut tagged: Vec<(usize, usize, String)> = annotations_with_positions
        .into_iter()
        .enumerate()
        .map(|(orig_idx, (line_idx, dsl))| (line_idx, orig_idx, dsl))
        .collect();

    // Sort descending by line_index for bottom-up insertion.
    // For same line_index, sort descending by original order so that
    // after bottom-up insertion the original order is preserved.
    tagged.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let annotations_with_positions: Vec<(usize, String)> =
        tagged.into_iter().map(|(li, _, dsl)| (li, dsl)).collect();

    for (line_idx, dsl) in annotations_with_positions {
        // Clamp: never insert inside frontmatter
        let insert_after = line_idx.max(frontmatter_end.unwrap_or(0));
        let pos = (insert_after + 1).min(lines.len());
        lines.insert(pos, String::new());
        lines.insert(pos + 1, dsl);
    }

    let mut result = lines.join("\n");
    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // detect_frontmatter_end tests
    // -----------------------------------------------------------------------

    #[test]
    fn detect_frontmatter_end_present() {
        let lines = vec![
            "---".to_string(),
            "title: Test".to_string(),
            "---".to_string(),
            "body".to_string(),
        ];
        assert_eq!(detect_frontmatter_end(&lines), Some(2));
    }

    #[test]
    fn detect_frontmatter_end_absent() {
        let lines = vec!["# Title".to_string(), "body".to_string()];
        assert_eq!(detect_frontmatter_end(&lines), None);
    }

    #[test]
    fn detect_frontmatter_end_no_closing() {
        let lines = vec![
            "---".to_string(),
            "title: Test".to_string(),
            "body".to_string(),
        ];
        assert_eq!(detect_frontmatter_end(&lines), None);
    }

    // -----------------------------------------------------------------------
    // insert_annotations_into_markdown tests
    // -----------------------------------------------------------------------

    #[test]
    fn insert_after_target_line() {
        let content = "line 0\nline 1\nline 2\nline 3\n";
        let annotations = vec![(1, "<!---[zot-1] n: | ann --->".to_string())];
        let result = insert_annotations_into_markdown(content, annotations);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[0], "line 0");
        assert_eq!(lines[1], "line 1");
        assert_eq!(lines[2], ""); // blank separator
        assert_eq!(lines[3], "<!---[zot-1] n: | ann --->");
        assert_eq!(lines[4], "line 2");
    }

    #[test]
    fn insert_multiple_preserves_order() {
        let content = "line 0\nline 1\nline 2\n";
        let annotations = vec![
            (0, "<!---[zot-1] n: | first --->".to_string()),
            (2, "<!---[zot-2] n: | second --->".to_string()),
        ];
        let result = insert_annotations_into_markdown(content, annotations);
        assert!(result.contains("first"));
        assert!(result.contains("second"));
        let pos1 = result.find("first").unwrap();
        let pos2 = result.find("second").unwrap();
        assert!(pos1 < pos2);
    }

    #[test]
    fn insert_respects_frontmatter() {
        let content = "---\ntitle: Test\n---\nline 3\nline 4\n";
        // Try to insert at line 0 (inside frontmatter) -- should be clamped to after frontmatter
        let annotations = vec![(0, "<!---[zot-1] n: | ann --->".to_string())];
        let result = insert_annotations_into_markdown(content, annotations);
        let lines: Vec<&str> = result.lines().collect();
        // Frontmatter should be intact
        assert_eq!(lines[0], "---");
        assert_eq!(lines[1], "title: Test");
        assert_eq!(lines[2], "---");
        // Annotation should appear after frontmatter (line 2 = closing ---),
        // so the inserted blank + annotation should be at lines 3 and 4
        assert_eq!(lines[3], "");
        assert_eq!(lines[4], "<!---[zot-1] n: | ann --->");
        assert_eq!(lines[5], "line 3");
    }

    #[test]
    fn insert_empty_annotations_returns_unchanged() {
        let content = "# Title\n\nBody text\n";
        let result = insert_annotations_into_markdown(content, vec![]);
        assert_eq!(result, content);
    }

    // -----------------------------------------------------------------------
    // Zotero Notes heading insertion tests
    // -----------------------------------------------------------------------

    #[test]
    fn notes_inserted_under_zotero_notes_heading() {
        // Simulate the note insertion logic: given markdown content and note DSLs,
        // the result should contain "## Zotero Notes" heading above the DSLs.
        let content = "# Title\n\nBody text here.\n";
        let note_dsls = vec!["<!---[zot-note-400] n: | note content --->".to_string()];

        let mut result = content.to_string();

        // Replicate the insertion logic from import_single_entry
        let mut notes_content = String::new();
        for dsl in &note_dsls {
            notes_content.push_str(dsl);
            notes_content.push_str("\n\n");
        }

        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str("\n\n## Zotero Notes\n\n");
        result.push_str(&notes_content);

        assert!(result.contains("## Zotero Notes"));
        assert!(result.contains("zot-note-400"));
        // Notes heading should appear after body text
        let heading_pos = result.find("## Zotero Notes").unwrap();
        let body_pos = result.find("Body text").unwrap();
        assert!(heading_pos > body_pos);
    }

    #[test]
    fn notes_heading_not_duplicated_on_reimport() {
        // If "## Zotero Notes" already exists, a second import should not add another heading.
        let content = "# Title\n\nBody\n\n## Zotero Notes\n\n<!---[zot-note-400] n: | existing --->\n\n";

        // Count occurrences of "## Zotero Notes" -- should be exactly 1
        let count = content.matches("## Zotero Notes").count();
        assert_eq!(count, 1, "Precondition: one heading");

        // The real logic checks `result.contains("## Zotero Notes")` and appends
        // without adding a new heading. Verify the pattern:
        assert!(content.contains("## Zotero Notes"));

        // If we were to add more notes, heading should not be duplicated
        let mut result = content.to_string();
        let new_dsl = "<!---[zot-note-401] n: | new note --->";

        // Replicate the "heading exists" branch
        let heading_pos = result.find("## Zotero Notes").unwrap();
        let after_heading = heading_pos + "## Zotero Notes".len();
        let next_heading = result[after_heading..]
            .find("\n## ")
            .map(|p| after_heading + p);
        let insert_at = next_heading.unwrap_or(result.len());
        let mut insert_block = String::from("\n");
        insert_block.push_str(new_dsl);
        insert_block.push_str("\n\n");
        result.insert_str(insert_at, &insert_block);

        let final_count = result.matches("## Zotero Notes").count();
        assert_eq!(final_count, 1, "Should still have exactly one heading");
        assert!(result.contains("zot-note-401"));
        assert!(result.contains("zot-note-400"));
    }

    #[test]
    fn notes_inserted_before_unmatched_section() {
        let content =
            "# Title\n\nBody\n\n## Unmatched Zotero Annotations\n\n<!---[zot-1] n: | unmatched --->\n";

        let notes_heading_pos = content.find("## Zotero Notes");
        assert!(notes_heading_pos.is_none(), "Precondition: no notes heading yet");

        // Replicate the insertion logic for notes before "## Unmatched"
        let mut result = content.to_string();
        let insertion_point = result
            .find("\n## Unmatched Zotero Annotations")
            .or_else(|| result.find("## Unmatched Zotero Annotations"));

        let mut notes_block = String::new();
        notes_block.push_str("\n\n## Zotero Notes\n\n");
        notes_block.push_str("<!---[zot-note-500] n: | a note --->\n\n");

        if let Some(pos) = insertion_point {
            result.insert_str(pos, &notes_block);
        }

        let notes_pos = result.find("## Zotero Notes").unwrap();
        let unmatched_pos = result.find("## Unmatched Zotero Annotations").unwrap();
        assert!(
            notes_pos < unmatched_pos,
            "Notes section should appear before Unmatched section"
        );
    }

    // -----------------------------------------------------------------------
    // Same-line insertion order tests
    // -----------------------------------------------------------------------

    #[test]
    fn insert_same_line_preserves_original_order() {
        let content = "line 0\nline 1\nline 2\n";
        // Two annotations both targeting line 1, given in order A then B.
        let annotations = vec![
            (1, "<!---[zot-A] n: | first --->".to_string()),
            (1, "<!---[zot-B] n: | second --->".to_string()),
        ];
        let result = insert_annotations_into_markdown(content, annotations);
        let pos_a = result.find("zot-A").expect("should contain zot-A");
        let pos_b = result.find("zot-B").expect("should contain zot-B");
        assert!(
            pos_a < pos_b,
            "zot-A should appear before zot-B (original order), got:\n{}",
            result
        );
    }

    #[test]
    fn insert_same_line_three_annotations() {
        let content = "line 0\nline 1\n";
        let annotations = vec![
            (0, "<!---[zot-1] n: | first --->".to_string()),
            (0, "<!---[zot-2] n: | second --->".to_string()),
            (0, "<!---[zot-3] n: | third --->".to_string()),
        ];
        let result = insert_annotations_into_markdown(content, annotations);
        let pos1 = result.find("zot-1").unwrap();
        let pos2 = result.find("zot-2").unwrap();
        let pos3 = result.find("zot-3").unwrap();
        assert!(pos1 < pos2, "zot-1 before zot-2");
        assert!(pos2 < pos3, "zot-2 before zot-3");
    }
}
