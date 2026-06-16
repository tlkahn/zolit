use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ZolitError;
use crate::zotero::types::{ZoteroAnnotation, ZoteroChildNote};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ZoteroSyncManifest {
    #[serde(rename = "lastImport", default, skip_serializing_if = "Option::is_none")]
    pub last_import: Option<String>,

    #[serde(rename = "lastDbMtime", default, skip_serializing_if = "Option::is_none")]
    pub last_db_mtime: Option<u64>,

    #[serde(default)]
    pub entries: HashMap<String, SyncEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SyncEntryState {
    #[serde(rename = "importedIds", default)]
    pub imported_ids: Vec<String>,
}

fn manifest_path(md_dir: &Path) -> PathBuf {
    md_dir.join(".zolit-sync.json")
}

pub fn read_manifest(md_dir: &Path) -> ZoteroSyncManifest {
    let path = manifest_path(md_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => ZoteroSyncManifest::default(),
    }
}

pub fn write_manifest(md_dir: &Path, manifest: &ZoteroSyncManifest) -> crate::error::Result<()> {
    let path = manifest_path(md_dir);
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| ZolitError::Other(format!("failed to serialize manifest: {}", e)))?;
    std::fs::write(&path, json).map_err(|e| ZolitError::Io {
        source: e,
        path: path.clone(),
    })
}

pub fn zotero_db_mtime(db_path: &str) -> Option<u64> {
    std::fs::metadata(db_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

pub fn zotero_db_changed_since_last_import(
    db_path: &str,
    manifest: &ZoteroSyncManifest,
) -> bool {
    let current_mtime = zotero_db_mtime(db_path);
    match (current_mtime, manifest.last_db_mtime) {
        (Some(current), Some(last)) => current != last,
        _ => true,
    }
}

pub fn update_manifest_entry(
    manifest: &mut ZoteroSyncManifest,
    entry_key: &str,
    annotations: &[ZoteroAnnotation],
    child_notes: &[ZoteroChildNote],
) {
    let entry = manifest
        .entries
        .entry(entry_key.to_string())
        .or_insert_with(SyncEntryState::default);

    let mut id_set: HashSet<String> = entry.imported_ids.iter().cloned().collect();
    for ann in annotations {
        id_set.insert(format!("zot-{}", ann.item_id));
    }
    for note in child_notes {
        id_set.insert(format!("zot-note-{}", note.item_id));
    }
    let mut ids: Vec<String> = id_set.into_iter().collect();
    ids.sort();
    entry.imported_ids = ids;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_path() {
        let root = Path::new("/workspace");
        let path = manifest_path(root);
        assert_eq!(path, PathBuf::from("/workspace/.zolit-sync.json"));
    }

    #[test]
    fn test_read_manifest_absent_file_returns_default() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = read_manifest(dir.path());
        assert_eq!(manifest, ZoteroSyncManifest::default());
    }

    #[test]
    fn test_read_manifest_malformed_json_returns_default() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".zolit-sync.json"), "not valid json {{{").unwrap();
        let manifest = read_manifest(dir.path());
        assert_eq!(manifest, ZoteroSyncManifest::default());
    }

    #[test]
    fn test_write_and_read_manifest_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut manifest = ZoteroSyncManifest::default();
        manifest.last_import = Some("2024-01-01T00:00:00Z".to_string());
        manifest.last_db_mtime = Some(1704067200);
        manifest.entries.insert(
            "smith2024".to_string(),
            SyncEntryState {
                imported_ids: vec!["zot-301".to_string(), "zot-302".to_string()],
            },
        );

        write_manifest(dir.path(), &manifest).unwrap();
        let loaded = read_manifest(dir.path());
        assert_eq!(loaded, manifest);
    }

    #[test]
    fn test_write_manifest_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = ZoteroSyncManifest::default();
        write_manifest(dir.path(), &manifest).unwrap();
        assert!(dir.path().join(".zolit-sync.json").exists());
    }

    #[test]
    fn test_zotero_db_mtime_existing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.sqlite");
        std::fs::write(&file_path, "test").unwrap();
        let mtime = zotero_db_mtime(file_path.to_str().unwrap());
        assert!(mtime.is_some());
        assert!(mtime.unwrap() > 0);
    }

    #[test]
    fn test_zotero_db_mtime_nonexistent_file() {
        let mtime = zotero_db_mtime("/nonexistent/path/zotero.sqlite");
        assert!(mtime.is_none());
    }

    #[test]
    fn test_zotero_db_changed_detects_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.sqlite");
        std::fs::write(&file_path, "test").unwrap();

        let manifest = ZoteroSyncManifest {
            last_db_mtime: Some(12345),
            ..Default::default()
        };
        assert!(zotero_db_changed_since_last_import(
            file_path.to_str().unwrap(),
            &manifest,
        ));
    }

    #[test]
    fn test_zotero_db_changed_same_mtime_returns_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.sqlite");
        std::fs::write(&file_path, "test").unwrap();

        let current_mtime = zotero_db_mtime(file_path.to_str().unwrap()).unwrap();
        let manifest = ZoteroSyncManifest {
            last_db_mtime: Some(current_mtime),
            ..Default::default()
        };
        assert!(!zotero_db_changed_since_last_import(
            file_path.to_str().unwrap(),
            &manifest,
        ));
    }

    #[test]
    fn test_zotero_db_changed_no_previous_mtime_returns_true() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.sqlite");
        std::fs::write(&file_path, "test").unwrap();

        let manifest = ZoteroSyncManifest {
            last_db_mtime: None,
            ..Default::default()
        };
        assert!(zotero_db_changed_since_last_import(
            file_path.to_str().unwrap(),
            &manifest,
        ));
    }

    #[test]
    fn test_update_manifest_entry_adds_new_ids() {
        let mut manifest = ZoteroSyncManifest::default();
        let anns = vec![ZoteroAnnotation {
            item_id: 301,
            ann_type: 1,
            text: Some("text".to_string()),
            comment: None,
            color: None,
            page_label: None,
            sort_index: "00001|000001|00000".to_string(),
        }];
        let notes = vec![ZoteroChildNote {
            item_id: 400,
            html_content: "note".to_string(),
            title: None,
        }];

        update_manifest_entry(&mut manifest, "smith2024", &anns, &notes);

        let entry = manifest.entries.get("smith2024").unwrap();
        assert!(entry.imported_ids.contains(&"zot-301".to_string()));
        assert!(entry.imported_ids.contains(&"zot-note-400".to_string()));
    }

    #[test]
    fn test_update_manifest_entry_preserves_existing_ids() {
        let mut manifest = ZoteroSyncManifest::default();
        manifest.entries.insert(
            "smith2024".to_string(),
            SyncEntryState {
                imported_ids: vec!["zot-100".to_string()],
            },
        );

        let anns = vec![ZoteroAnnotation {
            item_id: 301,
            ann_type: 1,
            text: None,
            comment: None,
            color: None,
            page_label: None,
            sort_index: "00001|000001|00000".to_string(),
        }];

        update_manifest_entry(&mut manifest, "smith2024", &anns, &[]);

        let entry = manifest.entries.get("smith2024").unwrap();
        assert!(entry.imported_ids.contains(&"zot-100".to_string()));
        assert!(entry.imported_ids.contains(&"zot-301".to_string()));
    }

    #[test]
    fn test_update_manifest_entry_sorted_ids() {
        let mut manifest = ZoteroSyncManifest::default();
        let anns = vec![
            ZoteroAnnotation {
                item_id: 999,
                ann_type: 1,
                text: None,
                comment: None,
                color: None,
                page_label: None,
                sort_index: "00001|000001|00000".to_string(),
            },
            ZoteroAnnotation {
                item_id: 100,
                ann_type: 1,
                text: None,
                comment: None,
                color: None,
                page_label: None,
                sort_index: "00002|000001|00000".to_string(),
            },
        ];

        update_manifest_entry(&mut manifest, "test", &anns, &[]);

        let entry = manifest.entries.get("test").unwrap();
        let ids = &entry.imported_ids;
        for i in 1..ids.len() {
            assert!(ids[i - 1] <= ids[i], "IDs should be sorted");
        }
    }
}
