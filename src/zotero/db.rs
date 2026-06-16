use rusqlite::{params, Connection, OpenFlags};

use super::types::{ZoteroAnnotation, ZoteroChildNote};
use crate::error::ZolitError;
use crate::html::html_to_markdown;

pub(crate) fn encode_sqlite_uri_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len() * 2);
    for c in path.chars() {
        match c {
            '%' => encoded.push_str("%25"),
            '#' => encoded.push_str("%23"),
            '?' => encoded.push_str("%3F"),
            ' ' => encoded.push_str("%20"),
            _ => encoded.push(c),
        }
    }
    encoded
}

/// Open the Zotero SQLite database in read-only mode.
/// Uses URI filename with `?mode=ro` to avoid WAL lock contention
/// when Zotero is running.
pub(crate) fn open_zotero_db(db_path: &str) -> Result<Connection, ZolitError> {
    let uri_path = encode_sqlite_uri_path(db_path);
    Connection::open_with_flags(
        format!("file:{}?mode=ro", uri_path),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("database is locked") || msg.contains("SQLITE_BUSY") {
            ZolitError::Other(format!(
                "Zotero database at {} is locked — is Zotero running? \
                 Close Zotero or try again in a moment. ({})",
                db_path, e
            ))
        } else {
            ZolitError::DbOpen {
                path: db_path.to_string(),
                source: e,
            }
        }
    })
}

/// Query the Zotero database for annotations on the attachment with the given itemID.
///
/// Returns annotations of type 1 (highlight), 2 (note/sticky), 5 (underline),
/// and 6 (freetext), filtering out type 3 (image) and type 4 (ink/freehand)
/// since they cannot be meaningfully represented as text. Results are ordered
/// by `sortIndex` which encodes document position (page, y-position, character offset).
pub fn query_zotero_for_pdf(
    db_path: &str,
    att_item_id: i64,
) -> Result<Vec<ZoteroAnnotation>, ZolitError> {
    let conn = open_zotero_db(db_path)?;

    let mut stmt = conn
        .prepare(
            "SELECT
                ia.itemID,
                ia.type,
                ia.text,
                ia.comment,
                ia.color,
                ia.pageLabel,
                ia.sortIndex
            FROM itemAnnotations ia
            WHERE ia.parentItemID = ?1
              AND ia.type NOT IN (3, 4)
            ORDER BY ia.sortIndex ASC",
        )
        .map_err(|e| ZolitError::DbQuery(e))?;

    let rows = stmt
        .query_map(params![att_item_id], |row| {
            Ok(ZoteroAnnotation {
                item_id: row.get(0)?,
                ann_type: row.get(1)?,
                text: row.get(2)?,
                comment: row.get(3)?,
                color: row.get(4)?,
                page_label: row.get(5)?,
                sort_index: row.get(6)?,
            })
        })
        .map_err(|e| ZolitError::DbQuery(e))?;

    let mut annotations = Vec::new();
    for row in rows {
        annotations.push(row.map_err(|e| ZolitError::DbQuery(e))?);
    }
    Ok(annotations)
}

/// Resolve a PDF filename stem to its Zotero attachment and parent item IDs.
///
/// Returns `Some((attachment_item_id, parent_item_id))` if found, `None` otherwise.
/// Matching is case-insensitive. Orphan attachments (NULL parentItemID) are skipped.
pub fn resolve_pdf_in_zotero(
    db_path: &str,
    pdf_filename_stem: &str,
) -> Result<Option<(i64, i64)>, ZolitError> {
    let conn = open_zotero_db(db_path)?;

    // Match the exact filename after the "storage:" prefix or after the last "/".
    // Two patterns: "storage:<filename>.pdf" (Zotero stored files) or any path
    // ending in "/<filename>.pdf" (linked files). Both case-insensitive.
    let filename_with_ext = format!("{}.pdf", pdf_filename_stem);
    let mut stmt = conn
        .prepare(
            "SELECT
                att.itemID,
                att.parentItemID
            FROM itemAttachments att
            JOIN items i ON att.itemID = i.itemID
            WHERE i.itemTypeID = (SELECT itemTypeID FROM itemTypes WHERE typeName = 'attachment')
              AND att.contentType = 'application/pdf'
              AND (
                  LOWER(att.path) = LOWER('storage:' || ?1)
                  OR LOWER(SUBSTR(att.path, -LENGTH('/' || ?1))) = LOWER('/' || ?1)
              )
            LIMIT 1",
        )
        .map_err(|e| ZolitError::DbQuery(e))?;

    let mut rows = stmt
        .query(params![filename_with_ext])
        .map_err(|e| ZolitError::DbQuery(e))?;

    while let Some(row) = rows.next().map_err(|e| ZolitError::DbQuery(e))? {
        let att_id: i64 = row
            .get(0)
            .map_err(|e| ZolitError::DbQuery(e))?;
        let parent_id: Option<i64> = row
            .get(1)
            .map_err(|e| ZolitError::DbQuery(e))?;
        if let Some(pid) = parent_id {
            return Ok(Some((att_id, pid)));
        }
    }
    Ok(None)
}

/// Query child notes attached to a Zotero parent item.
///
/// HTML content is converted to Markdown. Results are ordered by item ID.
pub fn query_zotero_child_notes(
    db_path: &str,
    parent_item_id: i64,
) -> Result<Vec<ZoteroChildNote>, ZolitError> {
    let conn = open_zotero_db(db_path)?;

    let mut stmt = conn
        .prepare(
            "SELECT
                n.itemID,
                n.note,
                n.title
            FROM itemNotes n
            JOIN items i ON n.itemID = i.itemID
            WHERE n.parentItemID = ?1
              AND i.itemTypeID = (SELECT itemTypeID FROM itemTypes WHERE typeName = 'note')
            ORDER BY n.itemID ASC",
        )
        .map_err(|e| ZolitError::DbQuery(e))?;

    let rows = stmt
        .query_map(params![parent_item_id], |row| {
            let item_id: i64 = row.get(0)?;
            let note: Option<String> = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let html_content = html_to_markdown(note.as_deref().unwrap_or(""));
            Ok(ZoteroChildNote {
                item_id,
                html_content,
                title,
            })
        })
        .map_err(|e| ZolitError::DbQuery(e))?;

    let mut notes = Vec::new();
    for row in rows {
        notes.push(row.map_err(|e| ZolitError::DbQuery(e))?);
    }
    Ok(notes)
}

/// List all PDF attachments that have at least one annotation.
///
/// Returns `Vec<(filename_stem, attachment_item_id, parent_item_id)>`.
/// Only stored (`storage:`) and linked-file PDFs with a non-NULL parent are included.
pub fn query_all_annotated_pdfs(
    db_path: &str,
) -> Result<Vec<(String, i64, i64)>, ZolitError> {
    let conn = open_zotero_db(db_path)?;

    let mut stmt = conn
        .prepare(
            "SELECT
                att.path,
                att.itemID,
                att.parentItemID
            FROM itemAttachments att
            JOIN items i ON att.itemID = i.itemID
            JOIN itemAnnotations ia ON ia.parentItemID = att.itemID
            WHERE att.contentType = 'application/pdf'
              AND att.parentItemID IS NOT NULL
            GROUP BY att.itemID
            ORDER BY att.itemID ASC",
        )
        .map_err(|e| ZolitError::DbQuery(e))?;

    let rows = stmt
        .query_map([], |row| {
            let path: String = row.get(0)?;
            let att_id: i64 = row.get(1)?;
            let parent_id: i64 = row.get(2)?;
            Ok((path, att_id, parent_id))
        })
        .map_err(|e| ZolitError::DbQuery(e))?;

    let mut results = Vec::new();
    for row in rows {
        let (path, att_id, parent_id) = row.map_err(|e| ZolitError::DbQuery(e))?;

        // Extract filename stem from "storage:Foo.pdf" or "/path/to/Foo.pdf"
        let filename = if let Some(stripped) = path.strip_prefix("storage:") {
            stripped.to_string()
        } else {
            path.rsplit('/').next().unwrap_or(&path).to_string()
        };

        let stem = if filename.to_ascii_lowercase().ends_with(".pdf") {
            &filename[..filename.len() - 4]
        } else {
            &filename
        }
        .to_string();

        results.push((stem, att_id, parent_id));
    }
    Ok(results)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rusqlite::Connection;

    pub(crate) fn create_test_db() -> (tempfile::TempDir, String) {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let path_str = db_path.to_str().unwrap().to_string();

        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch(
            "
            CREATE TABLE itemTypes (
                itemTypeID INTEGER PRIMARY KEY,
                typeName TEXT NOT NULL UNIQUE
            );
            INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, 'attachment');
            INSERT INTO itemTypes (itemTypeID, typeName) VALUES (2, 'note');
            INSERT INTO itemTypes (itemTypeID, typeName) VALUES (3, 'journalArticle');

            CREATE TABLE items (
                itemID INTEGER PRIMARY KEY,
                itemTypeID INTEGER NOT NULL,
                FOREIGN KEY (itemTypeID) REFERENCES itemTypes(itemTypeID)
            );

            CREATE TABLE itemAttachments (
                itemID INTEGER PRIMARY KEY,
                parentItemID INTEGER,
                contentType TEXT,
                path TEXT,
                FOREIGN KEY (itemID) REFERENCES items(itemID)
            );

            CREATE TABLE itemAnnotations (
                itemID INTEGER PRIMARY KEY,
                parentItemID INTEGER NOT NULL,
                type INTEGER NOT NULL,
                text TEXT,
                comment TEXT,
                color TEXT,
                pageLabel TEXT,
                sortIndex TEXT NOT NULL,
                FOREIGN KEY (parentItemID) REFERENCES items(itemID)
            );

            CREATE TABLE itemNotes (
                itemID INTEGER PRIMARY KEY,
                parentItemID INTEGER,
                note TEXT,
                title TEXT,
                FOREIGN KEY (itemID) REFERENCES items(itemID)
            );
        ",
        )
        .unwrap();

        // -- Parent item (journal article) --
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (100, 3)",
            [],
        )
        .unwrap();

        // -- PDF attachment --
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (200, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemAttachments (itemID, parentItemID, contentType, path)
             VALUES (200, 100, 'application/pdf', 'storage:Smith2024_Deep_Learning.pdf')",
            [],
        )
        .unwrap();

        // -- Annotations on the PDF --

        // Type 1 = highlight (page 5)
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (301, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemAnnotations (itemID, parentItemID, type, text, comment, color, pageLabel, sortIndex)
             VALUES (301, 200, 1, 'highlighted text one', 'my comment', '#ffd400', '5', '00005|000100|00000')",
            [],
        )
        .unwrap();

        // Type 2 = note/sticky note (page 3)
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (302, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemAnnotations (itemID, parentItemID, type, text, comment, color, pageLabel, sortIndex)
             VALUES (302, 200, 2, NULL, 'sticky note text', '#ff6666', '3', '00003|000050|00000')",
            [],
        )
        .unwrap();

        // Type 3 = image (should be SKIPPED)
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (303, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemAnnotations (itemID, parentItemID, type, text, comment, color, pageLabel, sortIndex)
             VALUES (303, 200, 3, NULL, 'image region', '#00ff00', '7', '00007|000200|00000')",
            [],
        )
        .unwrap();

        // Type 4 = ink (should be SKIPPED)
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (304, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemAnnotations (itemID, parentItemID, type, text, comment, color, pageLabel, sortIndex)
             VALUES (304, 200, 4, NULL, 'freehand drawing', '#0000ff', '8', '00008|000300|00000')",
            [],
        )
        .unwrap();

        // Type 1 = another highlight (page 1, should appear first after sort)
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (305, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemAnnotations (itemID, parentItemID, type, text, comment, color, pageLabel, sortIndex)
             VALUES (305, 200, 1, 'highlighted text on page one', NULL, '#ffd400', '1', '00001|000020|00000')",
            [],
        )
        .unwrap();

        // -- Child notes on the parent item --
        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (400, 2)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemNotes (itemID, parentItemID, note, title)
             VALUES (400, 100, '<p>This is a <b>child note</b> with HTML.</p>', NULL)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO items (itemID, itemTypeID) VALUES (401, 2)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO itemNotes (itemID, parentItemID, note, title)
             VALUES (401, 100, '<p>Second note</p><p>with two paragraphs.</p>', 'My Note Title')",
            [],
        )
        .unwrap();

        drop(conn);
        (dir, path_str)
    }

    // -----------------------------------------------------------------------
    // query_zotero_for_pdf tests
    // -----------------------------------------------------------------------

    #[test]
    fn extracts_annotations_ordered_by_sort_index() {
        let (_dir, db_path) = create_test_db();
        let anns = query_zotero_for_pdf(&db_path, 200).unwrap();

        // Should have 3 annotations (types 1, 2, 1) -- types 3, 4 filtered out
        assert_eq!(anns.len(), 3);

        // Ordered by sortIndex: page 1, page 3, page 5
        assert_eq!(anns[0].page_label.as_deref(), Some("1"));
        assert_eq!(
            anns[0].text.as_deref(),
            Some("highlighted text on page one")
        );
        assert_eq!(anns[0].sort_index, "00001|000020|00000");

        assert_eq!(anns[1].page_label.as_deref(), Some("3"));
        assert_eq!(anns[1].ann_type, 2); // sticky note
        assert_eq!(anns[1].comment.as_deref(), Some("sticky note text"));

        assert_eq!(anns[2].page_label.as_deref(), Some("5"));
        assert_eq!(anns[2].text.as_deref(), Some("highlighted text one"));
        assert_eq!(anns[2].comment.as_deref(), Some("my comment"));
    }

    #[test]
    fn filters_out_image_and_ink_annotations() {
        let (_dir, db_path) = create_test_db();
        let anns = query_zotero_for_pdf(&db_path, 200).unwrap();

        for ann in &anns {
            assert!(
                ann.ann_type != 3,
                "image annotation (type 3) should be filtered out"
            );
            assert!(
                ann.ann_type != 4,
                "ink annotation (type 4) should be filtered out"
            );
        }
    }

    #[test]
    fn no_annotations_returns_empty_vec() {
        let (_dir, db_path) = create_test_db();
        let anns = query_zotero_for_pdf(&db_path, 99999).unwrap();
        assert!(anns.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_pdf_in_zotero tests
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_pdf_case_insensitive() {
        let (_dir, db_path) = create_test_db();

        // Exact stem
        let result = resolve_pdf_in_zotero(&db_path, "Smith2024_Deep_Learning").unwrap();
        assert_eq!(result, Some((200, 100)));

        // Lowercase stem
        let result = resolve_pdf_in_zotero(&db_path, "smith2024_deep_learning").unwrap();
        assert_eq!(result, Some((200, 100)));

        // Uppercase stem
        let result = resolve_pdf_in_zotero(&db_path, "SMITH2024_DEEP_LEARNING").unwrap();
        assert_eq!(result, Some((200, 100)));

        // Non-existent stem
        let result = resolve_pdf_in_zotero(&db_path, "nonexistent_paper").unwrap();
        assert_eq!(result, None);
    }

    // -----------------------------------------------------------------------
    // query_zotero_child_notes tests
    // -----------------------------------------------------------------------

    #[test]
    fn queries_child_notes() {
        let (_dir, db_path) = create_test_db();
        let notes = query_zotero_child_notes(&db_path, 100).unwrap();

        assert_eq!(notes.len(), 2);

        // First note -- HTML converted to markdown (bold preserved)
        assert_eq!(notes[0].item_id, 400);
        assert!(
            !notes[0].html_content.contains('<'),
            "HTML tags should be stripped: {}",
            notes[0].html_content
        );
        assert!(notes[0].html_content.contains("**child note**"));
        assert_eq!(notes[0].title, None);

        // Second note -- has title, paragraphs separated by blank line
        assert_eq!(notes[1].item_id, 401);
        assert_eq!(notes[1].title.as_deref(), Some("My Note Title"));
        assert!(notes[1].html_content.contains("Second note"));
        assert!(notes[1].html_content.contains("two paragraphs"));
        // Two <p> tags should produce paragraph separation
        assert!(
            notes[1].html_content.contains("\n\n"),
            "Paragraphs should be separated by blank line: {:?}",
            notes[1].html_content
        );
    }

    #[test]
    fn no_child_notes_returns_empty_vec() {
        let (_dir, db_path) = create_test_db();
        let notes = query_zotero_child_notes(&db_path, 999).unwrap();
        assert!(notes.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_pdf exact match (no LIKE wildcards)
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_pdf_does_not_match_substring() {
        // Create a DB with a PDF named "TrainingAI.pdf" and check that
        // searching for stem "AI" does NOT match it (old LIKE would match).
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let path_str = db_path.to_str().unwrap().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName TEXT NOT NULL UNIQUE);
            INSERT INTO itemTypes VALUES (1, 'attachment');
            CREATE TABLE items (itemID INTEGER PRIMARY KEY, itemTypeID INTEGER NOT NULL);
            INSERT INTO items VALUES (10, 1);
            CREATE TABLE itemAttachments (
                itemID INTEGER PRIMARY KEY, parentItemID INTEGER,
                contentType TEXT, path TEXT
            );
            INSERT INTO itemAttachments VALUES (10, 5, 'application/pdf', 'storage:TrainingAI.pdf');
            ",
        )
        .unwrap();
        drop(conn);

        // "AI" should NOT match "TrainingAI.pdf"
        let result = resolve_pdf_in_zotero(&path_str, "AI").unwrap();
        assert_eq!(result, None, "short stem 'AI' should not match 'TrainingAI.pdf'");

        // "TrainingAI" SHOULD match
        let result = resolve_pdf_in_zotero(&path_str, "TrainingAI").unwrap();
        assert_eq!(result, Some((10, 5)));
    }

    // -----------------------------------------------------------------------
    // resolve_pdf linked file path
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_pdf_linked_file_path() {
        // Test that linked files (paths with "/" not "storage:") also match
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let path_str = db_path.to_str().unwrap().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName TEXT NOT NULL UNIQUE);
            INSERT INTO itemTypes VALUES (1, 'attachment');
            CREATE TABLE items (itemID INTEGER PRIMARY KEY, itemTypeID INTEGER NOT NULL);
            INSERT INTO items VALUES (20, 1);
            CREATE TABLE itemAttachments (
                itemID INTEGER PRIMARY KEY, parentItemID INTEGER,
                contentType TEXT, path TEXT
            );
            INSERT INTO itemAttachments VALUES (20, 10, 'application/pdf', '/home/user/Papers/MyPaper.pdf');
            ",
        )
        .unwrap();
        drop(conn);

        let result = resolve_pdf_in_zotero(&path_str, "MyPaper").unwrap();
        assert_eq!(result, Some((20, 10)));
    }

    // -----------------------------------------------------------------------
    // query_zotero_for_pdf by att_id
    // -----------------------------------------------------------------------

    #[test]
    fn query_by_att_id_returns_correct_annotations() {
        let (_dir, db_path) = create_test_db();
        // att_id 200 has 3 valid annotations (types 1, 2, 1; types 3, 4 filtered)
        let anns = query_zotero_for_pdf(&db_path, 200).unwrap();
        assert_eq!(anns.len(), 3);
        // Different att_id returns empty
        let anns2 = query_zotero_for_pdf(&db_path, 100).unwrap();
        assert!(anns2.is_empty());
    }

    // -----------------------------------------------------------------------
    // encode_sqlite_uri_path tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_encode_sqlite_uri_path_no_specials() {
        assert_eq!(
            encode_sqlite_uri_path("/usr/local/zotero.sqlite"),
            "/usr/local/zotero.sqlite"
        );
    }

    #[test]
    fn test_encode_sqlite_uri_path_space() {
        assert_eq!(
            encode_sqlite_uri_path("/path with spaces/db.sqlite"),
            "/path%20with%20spaces/db.sqlite"
        );
    }

    #[test]
    fn test_encode_sqlite_uri_path_hash() {
        assert_eq!(
            encode_sqlite_uri_path("/path/#fragment/db.sqlite"),
            "/path/%23fragment/db.sqlite"
        );
    }

    #[test]
    fn test_encode_sqlite_uri_path_question_mark() {
        assert_eq!(
            encode_sqlite_uri_path("/path/what?/db.sqlite"),
            "/path/what%3F/db.sqlite"
        );
    }

    #[test]
    fn test_encode_sqlite_uri_path_percent() {
        assert_eq!(
            encode_sqlite_uri_path("/path/100%/db.sqlite"),
            "/path/100%25/db.sqlite"
        );
    }

    #[test]
    fn test_encode_sqlite_uri_path_multiple_specials() {
        assert_eq!(
            encode_sqlite_uri_path("/a b/c#d/e?f/100%"),
            "/a%20b/c%23d/e%3Ff/100%25"
        );
    }

    #[test]
    fn test_encode_sqlite_uri_path_non_ascii() {
        assert_eq!(
            encode_sqlite_uri_path("/home/user/Référence/zotero.sqlite"),
            "/home/user/Référence/zotero.sqlite"
        );
    }

    // -----------------------------------------------------------------------
    // query_all_annotated_pdfs tests
    // -----------------------------------------------------------------------

    #[test]
    fn query_all_annotated_pdfs_returns_annotated_pdf() {
        let (_dir, db_path) = create_test_db();
        let pdfs = query_all_annotated_pdfs(&db_path).unwrap();

        // The test DB has one PDF attachment (id=200) with annotations
        assert_eq!(pdfs.len(), 1);
        assert_eq!(pdfs[0].0, "Smith2024_Deep_Learning");
        assert_eq!(pdfs[0].1, 200); // att_item_id
        assert_eq!(pdfs[0].2, 100); // parent_item_id
    }

    #[test]
    fn query_all_annotated_pdfs_excludes_unannotated() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let path_str = db_path.to_str().unwrap().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName TEXT NOT NULL UNIQUE);
            INSERT INTO itemTypes VALUES (1, 'attachment');
            INSERT INTO itemTypes VALUES (3, 'journalArticle');
            CREATE TABLE items (itemID INTEGER PRIMARY KEY, itemTypeID INTEGER NOT NULL);
            INSERT INTO items VALUES (100, 3);
            INSERT INTO items VALUES (200, 1);
            CREATE TABLE itemAttachments (
                itemID INTEGER PRIMARY KEY, parentItemID INTEGER,
                contentType TEXT, path TEXT
            );
            INSERT INTO itemAttachments VALUES (200, 100, 'application/pdf', 'storage:NoPaper.pdf');
            CREATE TABLE itemAnnotations (
                itemID INTEGER PRIMARY KEY, parentItemID INTEGER NOT NULL,
                type INTEGER NOT NULL, text TEXT, comment TEXT,
                color TEXT, pageLabel TEXT, sortIndex TEXT NOT NULL
            );
            -- No annotations inserted for this PDF
            ",
        )
        .unwrap();
        drop(conn);

        let pdfs = query_all_annotated_pdfs(&path_str).unwrap();
        assert!(pdfs.is_empty(), "PDF with no annotations should not appear");
    }
}
