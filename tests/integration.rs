use rusqlite::Connection;
use zolit::pipeline::sync_all;

fn create_test_db() -> (tempfile::TempDir, String) {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("zotero.sqlite");
    let path_str = db_path.to_str().unwrap().to_string();

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "
        CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName TEXT NOT NULL UNIQUE);
        INSERT INTO itemTypes VALUES (1, 'attachment');
        INSERT INTO itemTypes VALUES (2, 'note');
        INSERT INTO itemTypes VALUES (3, 'journalArticle');

        CREATE TABLE items (itemID INTEGER PRIMARY KEY, itemTypeID INTEGER NOT NULL);
        CREATE TABLE itemAttachments (itemID INTEGER PRIMARY KEY, parentItemID INTEGER, contentType TEXT, path TEXT);
        CREATE TABLE itemAnnotations (itemID INTEGER PRIMARY KEY, parentItemID INTEGER NOT NULL, type INTEGER NOT NULL, text TEXT, comment TEXT, color TEXT, pageLabel TEXT, sortIndex TEXT NOT NULL);
        CREATE TABLE itemNotes (itemID INTEGER PRIMARY KEY, parentItemID INTEGER, note TEXT, title TEXT);

        INSERT INTO items VALUES (100, 3);
        INSERT INTO items VALUES (200, 1);
        INSERT INTO itemAttachments VALUES (200, 100, 'application/pdf', 'storage:Smith2024_Deep_Learning.pdf');

        INSERT INTO items VALUES (301, 1);
        INSERT INTO itemAnnotations VALUES (301, 200, 1, 'highlighted text one', 'my comment', '#ffd400', '5', '00005|000100|00000');
        INSERT INTO items VALUES (302, 1);
        INSERT INTO itemAnnotations VALUES (302, 200, 2, NULL, 'sticky note text', '#ff6666', '3', '00003|000050|00000');
        INSERT INTO items VALUES (305, 1);
        INSERT INTO itemAnnotations VALUES (305, 200, 1, 'highlighted text on page one', NULL, '#ffd400', '1', '00001|000020|00000');

        INSERT INTO items VALUES (400, 2);
        INSERT INTO itemNotes VALUES (400, 100, '<p>A <b>child note</b>.</p>', NULL);
        ",
    )
    .unwrap();
    drop(conn);
    (dir, path_str)
}

#[test]
fn sync_all_inserts_and_dedup() {
    let (_db_dir, db_path) = create_test_db();
    let md_dir = tempfile::TempDir::new().unwrap();

    let md_content =
        "---\ntitle: Test\n---\n\nhighlighted text on page one\n\nsome other paragraph\n\nhighlighted text one\n";
    std::fs::write(
        md_dir.path().join("Smith2024_Deep_Learning.md"),
        md_content,
    )
    .unwrap();

    let result1 = sync_all(&db_path, md_dir.path(), 0.4, false).unwrap();
    assert!(
        result1.total_inserted > 0,
        "first sync should insert annotations"
    );
    assert_eq!(result1.entries_processed, 1);

    let output =
        std::fs::read_to_string(md_dir.path().join("Smith2024_Deep_Learning.md")).unwrap();
    assert!(output.contains("zot-"), "output should contain annotation IDs");

    let result2 = sync_all(&db_path, md_dir.path(), 0.4, false).unwrap();
    assert_eq!(
        result2.total_inserted, 0,
        "second sync should not insert duplicates"
    );
}

#[test]
fn sync_dry_run_does_not_modify() {
    let (_db_dir, db_path) = create_test_db();
    let md_dir = tempfile::TempDir::new().unwrap();

    let md_content = "---\ntitle: Test\n---\n\nhighlighted text on page one\n";
    let md_path = md_dir.path().join("Smith2024_Deep_Learning.md");
    std::fs::write(&md_path, md_content).unwrap();

    let result = sync_all(&db_path, md_dir.path(), 0.4, true).unwrap();
    assert_eq!(result.entries_processed, 1);

    let output = std::fs::read_to_string(&md_path).unwrap();
    assert_eq!(output, md_content, "dry run should not modify files");
    assert!(
        !md_dir.path().join(".zolit-sync.json").exists(),
        "dry run should not create manifest"
    );
}

#[test]
fn sync_no_companion_skips_gracefully() {
    let (_db_dir, db_path) = create_test_db();
    let md_dir = tempfile::TempDir::new().unwrap();

    let result = sync_all(&db_path, md_dir.path(), 0.4, false).unwrap();
    assert_eq!(result.entries_processed, 0);
    assert!(result.errors.is_empty());
}
