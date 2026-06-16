use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroAnnotation {
    pub item_id: i64,
    pub ann_type: i32,
    pub text: Option<String>,
    pub comment: Option<String>,
    pub color: Option<String>,
    pub page_label: Option<String>,
    pub sort_index: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroChildNote {
    pub item_id: i64,
    pub html_content: String,
    pub title: Option<String>,
}
