use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ZolitError {
    #[error("failed to open Zotero database at {path}: {source}")]
    DbOpen {
        path: String,
        source: rusqlite::Error,
    },

    #[error("database query failed: {0}")]
    DbQuery(#[from] rusqlite::Error),

    #[error("I/O error: {source} ({path})")]
    Io {
        source: std::io::Error,
        path: PathBuf,
    },

    #[error("I/O error: {0}")]
    IoPlain(#[from] std::io::Error),

    #[error("failed to parse manifest at {path}: {source}")]
    ManifestParse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("no companion markdown found for \"{stem}\"")]
    NoCompanion { stem: String },

    #[error("no matching PDF found in Zotero for \"{stem}\"")]
    NoZoteroMatch { stem: String },

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ZolitError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_no_companion() {
        let e = ZolitError::NoCompanion {
            stem: "Smith2024".into(),
        };
        assert_eq!(
            e.to_string(),
            "no companion markdown found for \"Smith2024\""
        );
    }

    #[test]
    fn display_no_zotero_match() {
        let e = ZolitError::NoZoteroMatch {
            stem: "Jones2023".into(),
        };
        assert_eq!(
            e.to_string(),
            "no matching PDF found in Zotero for \"Jones2023\""
        );
    }

    #[test]
    fn display_other() {
        let e = ZolitError::Other("something went wrong".into());
        assert_eq!(e.to_string(), "something went wrong");
    }
}
