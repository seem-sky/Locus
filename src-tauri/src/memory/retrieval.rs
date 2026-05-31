use super::models::{
    MemoryCategory, MemoryRetrieveHit, MemoryRetrieveOptions, DEFAULT_RETRIEVE_LIMIT,
    DEFAULT_TOKEN_BUDGET,
};
use super::store::MemoryStoreState;

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.len() != a.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (left, right) in a.iter().zip(b.iter()) {
        dot += left * right;
        norm_a += left * left;
        norm_b += right * right;
    }
    if norm_a <= f32::EPSILON || norm_b <= f32::EPSILON {
        return 0.0;
    }
    (dot / (norm_a.sqrt() * norm_b.sqrt())).clamp(0.0, 1.0)
}

pub fn keyword_overlap_score(query: &str, content: &str, tags: &[String]) -> f32 {
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return 0.0;
    }
    let mut haystack_tokens: std::collections::HashSet<String> =
        tokenize(content).into_iter().collect();
    for tag in tags {
        haystack_tokens.extend(tokenize(tag));
    }
    if haystack_tokens.is_empty() {
        return 0.0;
    }
    let hits = query_tokens
        .iter()
        .filter(|token| haystack_tokens.contains(*token))
        .count();
    (hits as f32 / query_tokens.len() as f32).clamp(0.0, 1.0)
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|segment| segment.len() >= 2)
        .map(|segment| segment.to_string())
        .collect()
}

pub fn retrieve_entries(
    store: &MemoryStoreState,
    working_dir: &str,
    _app_storage_dir: Option<&std::path::Path>,
    options: &MemoryRetrieveOptions,
    _query_embedding: Option<&[f32]>,
    _entry_embeddings: &[(String, super::models::MemoryScope, Vec<f32>)],
) -> Result<Vec<MemoryRetrieveHit>, String> {
    let mut normalized = options.clone();
    if normalized.limit.is_none() {
        normalized.limit = Some(DEFAULT_RETRIEVE_LIMIT);
    }
    if normalized.token_budget.is_none() {
        normalized.token_budget = Some(DEFAULT_TOKEN_BUDGET);
    }
    store.retrieve(working_dir, &normalized)
}

pub fn build_relevant_memory_prefix(hits: &[MemoryRetrieveHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }

    let mut grouped: Vec<(MemoryCategory, Vec<String>)> = Vec::new();
    for hit in hits {
        let line = format!("- {}", hit.entry.content.trim());
        if let Some((_, lines)) = grouped.iter_mut().find(|(cat, _)| *cat == hit.entry.category) {
            lines.push(line);
        } else {
            grouped.push((hit.entry.category, vec![line]));
        }
    }

    let mut body = String::from("<relevant-memories>\n");
    for (category, lines) in grouped {
        body.push_str(&format!("### {}\n", category_label(category)));
        for line in lines {
            body.push_str(&line);
            body.push('\n');
        }
        body.push('\n');
    }
    body.push_str("</relevant-memories>");
    body
}

fn category_label(category: MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::User => "User",
        MemoryCategory::Feedback => "Feedback",
        MemoryCategory::Topic => "Topic",
        MemoryCategory::Reference => "Reference",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_overlap_prefers_matching_terms() {
        let score = keyword_overlap_score("unity scene graph", "Unity scene graph setup", &[]);
        assert!(score > 0.5);
    }
}
