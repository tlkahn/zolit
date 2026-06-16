#![cfg(feature = "llm")]

use std::collections::HashMap;

use crate::dsl::generate::ann_type_name;
use crate::zotero::types::ZoteroAnnotation;

pub const LLM_PLACEMENT_SYSTEM_PROMPT: &str = "\
You are a document analyst. You will receive numbered paragraphs from an OCR'd academic document, \
and a set of annotations that could not be automatically matched to their location in the document.\n\
\n\
For each annotation (labeled ANN_0, ANN_1, etc.), determine which paragraph it most likely belongs \
to based on semantic similarity, topic overlap, or contextual clues. Return a JSON object mapping \
annotation labels to paragraph numbers. Use -1 if an annotation genuinely cannot be placed.\n\
\n\
Return ONLY a JSON object, no explanation. Example: {\"ANN_0\": 3, \"ANN_1\": 7, \"ANN_2\": -1}";

const LLM_BATCH_SIZE: usize = 10;
const LLM_MAX_SINGLE_BATCH: usize = 20;

pub fn build_llm_placement_prompt(
    unmatched: &[(usize, &ZoteroAnnotation)],
    paragraphs: &[(String, usize)],
) -> Option<String> {
    if unmatched.is_empty() || paragraphs.is_empty() {
        return None;
    }

    let mut prompt = String::from("## Document Paragraphs\n\n");
    for (i, (text, _)) in paragraphs.iter().enumerate() {
        let truncated: String = text.chars().take(200).collect();
        prompt.push_str(&format!("P{}: {}\n", i, truncated));
    }

    prompt.push_str("\n## Unmatched Annotations\n\n");
    let mut ann_count = 0;
    for (label_idx, (_, ann)) in unmatched.iter().enumerate() {
        let type_name = ann_type_name(ann.ann_type);
        let page_info = ann
            .page_label
            .as_deref()
            .filter(|p| !p.is_empty())
            .map(|p| format!("p.{}", p))
            .unwrap_or_default();

        let text_preview: String = ann
            .text
            .as_deref()
            .or(ann.comment.as_deref())
            .unwrap_or("")
            .chars()
            .take(200)
            .collect();

        prompt.push_str(&format!(
            "ANN_{}: [{}, {}] \"{}\"\n",
            label_idx, type_name, page_info, text_preview
        ));
        ann_count += 1;
    }

    if ann_count == 0 {
        return None;
    }

    Some(prompt)
}

pub fn parse_llm_placement_response(
    response: &str,
    paragraphs: &[(String, usize)],
) -> HashMap<usize, usize> {
    let mut result = HashMap::new();

    let trimmed = response.trim();
    let json_str = if trimmed.starts_with("```") {
        let after_fence = if let Some(pos) = trimmed.find('\n') {
            &trimmed[pos + 1..]
        } else {
            trimmed.trim_start_matches('`')
        };
        after_fence
            .trim_end()
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };

    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return result,
    };

    let obj = match parsed.as_object() {
        Some(o) => o,
        None => return result,
    };

    for (key, value) in obj {
        let ann_idx = if key.starts_with("ANN_") {
            key[4..].parse::<usize>().ok()
        } else {
            None
        };

        let para_idx = value.as_i64();

        if let (Some(ann_idx), Some(para_idx)) = (ann_idx, para_idx) {
            if para_idx < 0 {
                continue;
            }
            let para_idx = para_idx as usize;
            if para_idx < paragraphs.len() {
                let line_idx = paragraphs[para_idx].1;
                result.insert(ann_idx, line_idx);
            }
        }
    }

    result
}

pub async fn llm_find_positions(
    unmatched: &[(usize, &ZoteroAnnotation)],
    paragraphs: &[(String, usize)],
    model: &str,
    api_key: &str,
    base_url: &str,
) -> Result<HashMap<usize, usize>, String> {
    if unmatched.is_empty() {
        return Ok(HashMap::new());
    }

    if unmatched.len() <= LLM_MAX_SINGLE_BATCH {
        return llm_find_positions_batch(unmatched, paragraphs, model, api_key, base_url).await;
    }

    let mut all_results = HashMap::new();
    for (chunk_idx, chunk) in unmatched.chunks(LLM_BATCH_SIZE).enumerate() {
        match llm_find_positions_batch(chunk, paragraphs, model, api_key, base_url).await {
            Ok(batch_results) => {
                let offset = chunk_idx * LLM_BATCH_SIZE;
                for (label_idx, line_idx) in batch_results {
                    all_results.insert(offset + label_idx, line_idx);
                }
            }
            Err(e) => {
                tracing::warn!("LLM batch {} failed, skipping: {}", chunk_idx, e);
            }
        }
    }

    Ok(all_results)
}

async fn llm_find_positions_batch(
    unmatched: &[(usize, &ZoteroAnnotation)],
    paragraphs: &[(String, usize)],
    model: &str,
    api_key: &str,
    base_url: &str,
) -> Result<HashMap<usize, usize>, String> {
    let user_prompt = build_llm_placement_prompt(unmatched, paragraphs)
        .ok_or_else(|| "No annotations to place".to_string())?;

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": LLM_PLACEMENT_SYSTEM_PROMPT},
            {"role": "user", "content": user_prompt}
        ],
        "max_tokens": 500,
        "temperature": 0.0
    });

    let resp = client
        .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("LLM API returned {}: {}", status, text));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "No content in LLM response".to_string())?;

    Ok(parse_llm_placement_response(content, paragraphs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_llm_placement_prompt_basic() {
        let ann = ZoteroAnnotation {
            item_id: 1,
            ann_type: 1,
            text: Some("test text".to_string()),
            comment: None,
            color: None,
            page_label: Some("5".to_string()),
            sort_index: "00005|000100|00000".to_string(),
        };
        let unmatched = vec![(0, &ann)];
        let paragraphs = vec![
            ("paragraph zero".to_string(), 0),
            ("paragraph one".to_string(), 5),
        ];
        let prompt = build_llm_placement_prompt(&unmatched, &paragraphs);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("P0: paragraph zero"));
        assert!(p.contains("P1: paragraph one"));
        assert!(p.contains("ANN_0:"));
        assert!(p.contains("test text"));
    }

    #[test]
    fn test_build_llm_placement_prompt_empty() {
        let paragraphs = vec![("text".to_string(), 0)];
        assert!(build_llm_placement_prompt(&[], &paragraphs).is_none());
    }

    #[test]
    fn test_parse_llm_response_valid() {
        let paragraphs = vec![
            ("p0".to_string(), 10),
            ("p1".to_string(), 20),
            ("p2".to_string(), 30),
        ];
        let response = r#"{"ANN_0": 1, "ANN_1": 2}"#;
        let result = parse_llm_placement_response(response, &paragraphs);
        assert_eq!(result.get(&0), Some(&20));
        assert_eq!(result.get(&1), Some(&30));
    }

    #[test]
    fn test_parse_llm_response_with_fences() {
        let paragraphs = vec![("p0".to_string(), 5)];
        let response = "```json\n{\"ANN_0\": 0}\n```";
        let result = parse_llm_placement_response(response, &paragraphs);
        assert_eq!(result.get(&0), Some(&5));
    }

    #[test]
    fn test_parse_llm_response_negative_one() {
        let paragraphs = vec![("p0".to_string(), 5)];
        let response = r#"{"ANN_0": -1}"#;
        let result = parse_llm_placement_response(response, &paragraphs);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_llm_response_out_of_range() {
        let paragraphs = vec![("p0".to_string(), 5)];
        let response = r#"{"ANN_0": 99}"#;
        let result = parse_llm_placement_response(response, &paragraphs);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_llm_response_malformed() {
        let paragraphs = vec![("p0".to_string(), 5)];
        assert!(parse_llm_placement_response("not json", &paragraphs).is_empty());
    }
}
