use crate::session::models::{ChatMessage, MessageRole, ToolCallInfo};
use serde_json::{json, Value};

pub fn to_headroom_openai_messages(system_parts: &[&str], messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    let system = system_parts.join("\n\n");
    if !system.is_empty() {
        out.push(json!({
            "role": "system",
            "content": system,
        }));
    }

    for message in messages {
        match message.role {
            MessageRole::User => {
                out.push(json!({
                    "role": "user",
                    "content": message.content,
                }));
            }
            MessageRole::Assistant => {
                let mut value = json!({
                    "role": "assistant",
                    "content": message.content,
                });
                if let Some(tool_calls) = message.tool_calls.as_ref().filter(|calls| !calls.is_empty())
                {
                    value["tool_calls"] = Value::Array(
                        tool_calls
                            .iter()
                            .map(build_tool_call_value)
                            .collect(),
                    );
                }
                out.push(value);
            }
            MessageRole::Tool => {
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": message.tool_call_id.as_deref().unwrap_or(""),
                    "content": tool_message_content(message),
                }));
            }
        }
    }

    out
}

pub fn apply_compressed_messages(
    original: &[ChatMessage],
    compressed: &[Value],
) -> Result<Vec<ChatMessage>, String> {
    let payload: Vec<&Value> = compressed
        .iter()
        .filter(|value| {
            value
                .get("role")
                .and_then(|role| role.as_str())
                .is_some_and(|role| role != "system")
        })
        .collect();

    if payload.len() != original.len() {
        return Err(format!(
            "headroom message count mismatch: expected {}, got {}",
            original.len(),
            payload.len()
        ));
    }

    let mut updated = original.to_vec();
    for (index, compressed_message) in payload.into_iter().enumerate() {
        if let Some(content) = extract_message_content(compressed_message) {
            updated[index].content = content;
        }
    }
    Ok(updated)
}

fn build_tool_call_value(tool_call: &ToolCallInfo) -> Value {
    json!({
        "id": tool_call.id,
        "type": "function",
        "function": {
            "name": tool_call.name,
            "arguments": tool_call.arguments,
        }
    })
}

fn tool_message_content(message: &ChatMessage) -> String {
    let mut content = message.content.clone();
    if let Some(images) = message.images.as_ref().filter(|images| !images.is_empty()) {
        let note = format!(
            "\n\n[{} image attachment(s) omitted for Headroom context compression.]",
            images.len()
        );
        if content.trim().is_empty() {
            content = note.trim_start().to_string();
        } else {
            content.push_str(&note);
        }
    }
    content
}

fn extract_message_content(message: &Value) -> Option<String> {
    let content = message.get("content")?;
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let mut chunks = Vec::new();
            for part in parts {
                if part.get("type").and_then(|value| value.as_str()) == Some("text") {
                    if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                        if !text.is_empty() {
                            chunks.push(text.to_string());
                        }
                    }
                }
            }
            if chunks.is_empty() {
                None
            } else {
                Some(chunks.join("\n"))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::models::MessageRole;

    fn sample_message(id: &str, role: MessageRole, content: &str) -> ChatMessage {
        ChatMessage {
            id: id.to_string(),
            role,
            content: content.to_string(),
            created_at: 1,
            prompt_prefix: None,
            prompt_suffix: None,
            response_id: None,
            content_order: None,
            thinking_order: None,
            tool_calls: None,
            tool_call_id: None,
            images: None,
            asset_refs: None,
            thinking_content: None,
            thinking_duration: None,
            thinking_signature: None,
            knowledge_proposal: None,
            memory_proposal: None,
            render_parts: None,
        }
    }

    #[test]
    fn to_headroom_openai_messages_includes_system_and_roles() {
        let messages = vec![
            sample_message("u1", MessageRole::User, "hello"),
            sample_message("a1", MessageRole::Assistant, "hi"),
        ];
        let payload = to_headroom_openai_messages(&["rule one"], &messages);
        assert_eq!(payload.len(), 3);
        assert_eq!(payload[0]["role"], "system");
        assert_eq!(payload[1]["role"], "user");
        assert_eq!(payload[2]["role"], "assistant");
    }

    #[test]
    fn apply_compressed_messages_preserves_ids_and_updates_content() {
        let original = vec![
            sample_message("u1", MessageRole::User, "long user text"),
            sample_message("t1", MessageRole::Tool, "long tool output"),
        ];
        let compressed = vec![
            json!({"role": "system", "content": "rule one"}),
            json!({"role": "user", "content": "short user"}),
            json!({"role": "tool", "content": "short tool"}),
        ];
        let updated = apply_compressed_messages(&original, &compressed).expect("apply");
        assert_eq!(updated[0].id, "u1");
        assert_eq!(updated[0].content, "short user");
        assert_eq!(updated[1].id, "t1");
        assert_eq!(updated[1].content, "short tool");
    }

    #[test]
    fn apply_compressed_messages_rejects_length_mismatch() {
        let original = vec![sample_message("u1", MessageRole::User, "hello")];
        let compressed = vec![
            json!({"role": "system", "content": "rule"}),
            json!({"role": "user", "content": "a"}),
            json!({"role": "user", "content": "b"}),
        ];
        assert!(apply_compressed_messages(&original, &compressed).is_err());
    }
}
