use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::companion::scan_md_dir;
use crate::dsl::generate::{collect_unmatched_section, zotero_ann_to_dsl, zotero_note_to_dsl};
use crate::dsl::insert::insert_annotations_into_markdown;
use crate::dsl::scan::existing_zotero_ids;
use crate::error::{ZolitError, Result};
use crate::manifest::{
    read_manifest, update_manifest_entry, write_manifest, zotero_db_mtime, ZoteroSyncManifest,
};
use crate::matching::page_scope::find_page_ranges;
use crate::matching::pipeline::find_match_line_scoped;
use crate::zotero::db::{
    query_all_annotated_pdfs, query_zotero_child_notes, query_zotero_for_pdf,
    resolve_pdf_in_zotero,
};
use crate::zotero::types::{ZoteroAnnotation, ZoteroChildNote};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportResult {
    pub inserted: usize,
    pub unmatched: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BatchImportResult {
    pub entries_processed: usize,
    pub total_inserted: usize,
    pub total_unmatched: usize,
    pub total_skipped: usize,
    pub errors: Vec<(String, String)>,
}

pub fn sync_single_entry(
    pdf_stem: &str,
    companion_path: &Path,
    db_path: &str,
    threshold: f64,
    manifest: &ZoteroSyncManifest,
) -> Result<(ImportResult, Vec<ZoteroAnnotation>, Vec<ZoteroChildNote>)> {
    let (att_id, parent_id) = resolve_pdf_in_zotero(db_path, pdf_stem)?
        .ok_or_else(|| ZolitError::NoZoteroMatch {
            stem: pdf_stem.to_string(),
        })?;

    let annotations = query_zotero_for_pdf(db_path, att_id)?;
    let child_notes = query_zotero_child_notes(db_path, parent_id)?;

    let content = std::fs::read_to_string(companion_path).map_err(|e| ZolitError::Io {
        source: e,
        path: companion_path.to_path_buf(),
    })?;

    let existing_ids = existing_zotero_ids(&content);
    let manifest_ids: HashSet<String> = manifest
        .entries
        .get(pdf_stem)
        .map(|e| e.imported_ids.iter().cloned().collect())
        .unwrap_or_default();

    let new_anns: Vec<&ZoteroAnnotation> = annotations
        .iter()
        .filter(|a| {
            let zot_id = format!("zot-{}", a.item_id);
            !existing_ids.contains(&zot_id) && !manifest_ids.contains(&zot_id)
        })
        .collect();
    let new_notes: Vec<&ZoteroChildNote> = child_notes
        .iter()
        .filter(|n| {
            let zot_id = format!("zot-note-{}", n.item_id);
            !existing_ids.contains(&zot_id) && !manifest_ids.contains(&zot_id)
        })
        .collect();
    let skipped = (annotations.len() - new_anns.len()) + (child_notes.len() - new_notes.len());

    if new_anns.is_empty() && new_notes.is_empty() {
        return Ok((
            ImportResult {
                inserted: 0,
                unmatched: 0,
                skipped,
            },
            annotations,
            child_notes,
        ));
    }

    let lines: Vec<&str> = content.lines().collect();
    let page_ranges = find_page_ranges(&lines);
    let mut matched: Vec<(usize, String)> = Vec::new();
    let mut unmatched_anns: Vec<ZoteroAnnotation> = Vec::new();

    for ann in &new_anns {
        let dsl = zotero_ann_to_dsl(ann);
        let matchable_text = if ann.ann_type == 2 || ann.ann_type == 6 {
            None
        } else {
            ann.text.as_deref().filter(|t| !t.is_empty())
        };
        if let Some(text) = matchable_text {
            if let Some(line_idx) =
                find_match_line_scoped(text, &lines, threshold, ann.page_label.as_deref(), &page_ranges)
            {
                matched.push((line_idx, dsl));
            } else {
                unmatched_anns.push((*ann).clone());
            }
        } else {
            unmatched_anns.push((*ann).clone());
        }
    }

    let note_dsls: Vec<String> = new_notes.iter().map(|n| zotero_note_to_dsl(n)).collect();

    let inserted = matched.len() + note_dsls.len();
    let unmatched_count = unmatched_anns.len();

    let mut result = insert_annotations_into_markdown(&content, matched);

    if !note_dsls.is_empty() {
        let mut notes_content = String::new();
        for dsl in &note_dsls {
            notes_content.push_str(dsl);
            notes_content.push_str("\n\n");
        }

        if result.contains("## Zotero Notes") {
            let heading_pos = result.find("## Zotero Notes").unwrap();
            let after_heading = heading_pos + "## Zotero Notes".len();
            let next_heading = result[after_heading..]
                .find("\n## ")
                .map(|p| after_heading + p);
            let insert_at = next_heading.unwrap_or(result.len());
            let mut insert_block = String::from("\n");
            insert_block.push_str(&notes_content);
            result.insert_str(insert_at, &insert_block);
        } else {
            let insertion_point = result
                .find("\n## Unmatched Zotero Annotations")
                .or_else(|| result.find("## Unmatched Zotero Annotations"));

            let mut notes_block = String::new();
            notes_block.push_str("\n\n## Zotero Notes\n\n");
            notes_block.push_str(&notes_content);

            if let Some(pos) = insertion_point {
                result.insert_str(pos, &notes_block);
            } else {
                if !result.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str(&notes_block);
            }
        }
    }

    if !unmatched_anns.is_empty() {
        let section = collect_unmatched_section(&unmatched_anns);
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push('\n');
        result.push_str(&section);
        if !result.ends_with('\n') {
            result.push('\n');
        }
    }

    std::fs::write(companion_path, &result).map_err(|e| ZolitError::Io {
        source: e,
        path: companion_path.to_path_buf(),
    })?;

    Ok((
        ImportResult {
            inserted: inserted + unmatched_count,
            unmatched: unmatched_count,
            skipped,
        },
        annotations,
        child_notes,
    ))
}

pub fn sync_all(
    db_path: &str,
    md_dir: &Path,
    threshold: f64,
    dry_run: bool,
) -> Result<BatchImportResult> {
    let mut manifest = read_manifest(md_dir);
    let mut result = BatchImportResult::default();

    let annotated_pdfs = query_all_annotated_pdfs(db_path)?;
    let md_index = scan_md_dir(md_dir);

    for (pdf_stem, _att_id, _parent_id) in &annotated_pdfs {
        let companion = match md_index.get(&pdf_stem.to_lowercase()) {
            Some(path) => path.clone(),
            None => {
                tracing::debug!(stem = %pdf_stem, "no companion markdown found, skipping");
                continue;
            }
        };

        if dry_run {
            result.entries_processed += 1;
            tracing::info!(stem = %pdf_stem, companion = %companion.display(), "would sync");
            continue;
        }

        match sync_single_entry(pdf_stem, &companion, db_path, threshold, &manifest) {
            Ok((import_result, anns, notes)) => {
                result.entries_processed += 1;
                result.total_inserted += import_result.inserted;
                result.total_unmatched += import_result.unmatched;
                result.total_skipped += import_result.skipped;
                update_manifest_entry(&mut manifest, pdf_stem, &anns, &notes);
            }
            Err(e) => {
                result.entries_processed += 1;
                result.errors.push((pdf_stem.clone(), e.to_string()));
            }
        }
    }

    if !dry_run {
        manifest.last_import = Some(
            chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        );
        manifest.last_db_mtime = zotero_db_mtime(db_path);
        write_manifest(md_dir, &manifest)?;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zotero::db::tests::create_test_db;

    #[test]
    fn sync_inserts_annotations() {
        let (_db_dir, db_path) = create_test_db();
        let md_dir = tempfile::TempDir::new().unwrap();

        let md_content = "---\ntitle: Test\n---\n\nhighlighted text on page one\n\nsome other text\n\nhighlighted text one with my comment\n";
        std::fs::write(
            md_dir.path().join("Smith2024_Deep_Learning.md"),
            md_content,
        )
        .unwrap();

        let manifest = ZoteroSyncManifest::default();
        let companion = md_dir.path().join("Smith2024_Deep_Learning.md");
        let (result, _anns, _notes) = sync_single_entry(
            "Smith2024_Deep_Learning",
            &companion,
            &db_path,
            0.4,
            &manifest,
        )
        .unwrap();

        assert!(result.inserted > 0, "should insert annotations");

        let output = std::fs::read_to_string(&companion).unwrap();
        assert!(
            output.contains("zot-"),
            "output should contain zotero annotation IDs"
        );
    }

    #[test]
    fn sync_dedup_skips_existing() {
        let (_db_dir, db_path) = create_test_db();
        let md_dir = tempfile::TempDir::new().unwrap();

        let md_content = "---\ntitle: Test\n---\n\nhighlighted text on page one\n\nhighlighted text one\n";
        let companion = md_dir.path().join("Smith2024_Deep_Learning.md");
        std::fs::write(&companion, md_content).unwrap();

        let manifest = ZoteroSyncManifest::default();
        let (result1, anns, notes) = sync_single_entry(
            "Smith2024_Deep_Learning",
            &companion,
            &db_path,
            0.4,
            &manifest,
        )
        .unwrap();

        let mut manifest2 = manifest.clone();
        update_manifest_entry(&mut manifest2, "Smith2024_Deep_Learning", &anns, &notes);

        let (result2, _, _) = sync_single_entry(
            "Smith2024_Deep_Learning",
            &companion,
            &db_path,
            0.4,
            &manifest2,
        )
        .unwrap();

        assert!(result1.inserted > 0, "first sync should insert");
        assert_eq!(result2.inserted, 0, "second sync should skip all (dedup)");
        assert!(result2.skipped > 0, "second sync should report skipped");
    }

    #[test]
    fn sync_no_zotero_match() {
        let (_db_dir, db_path) = create_test_db();
        let md_dir = tempfile::TempDir::new().unwrap();

        let companion = md_dir.path().join("NonexistentPaper.md");
        std::fs::write(&companion, "# Nothing").unwrap();

        let manifest = ZoteroSyncManifest::default();
        let err = sync_single_entry(
            "NonexistentPaper",
            &companion,
            &db_path,
            0.4,
            &manifest,
        );
        assert!(err.is_err());
    }
}
