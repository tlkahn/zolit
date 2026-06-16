use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn scan_md_dir(md_dir: &Path) -> HashMap<String, PathBuf> {
    let mut index = HashMap::new();
    for entry in WalkDir::new(md_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                index
                    .entry(stem.to_lowercase())
                    .or_insert_with(|| path.to_path_buf());
            }
        }
    }
    index
}

pub fn find_companion(pdf_stem: &str, md_dir: &Path) -> Option<PathBuf> {
    let index = scan_md_dir(md_dir);
    index.get(&pdf_stem.to_lowercase()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Smith2024.md"), "# Smith").unwrap();
        let result = find_companion("Smith2024", dir.path());
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("Smith2024.md"));
    }

    #[test]
    fn case_insensitive_match() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Smith2024.md"), "# Smith").unwrap();
        let result = find_companion("smith2024", dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn no_match_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Smith2024.md"), "# Smith").unwrap();
        assert!(find_companion("nonexistent", dir.path()).is_none());
    }

    #[test]
    fn multiple_md_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Smith2024.md"), "").unwrap();
        std::fs::write(dir.path().join("Jones2023.md"), "").unwrap();
        std::fs::write(dir.path().join("Wang2022.md"), "").unwrap();
        assert!(find_companion("Jones2023", dir.path()).is_some());
        assert!(find_companion("Wang2022", dir.path()).is_some());
        assert!(find_companion("Missing", dir.path()).is_none());
    }

    #[test]
    fn nested_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let sub = dir.path().join("subdir");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("Deep2024.md"), "").unwrap();
        assert!(find_companion("Deep2024", dir.path()).is_some());
    }
}
