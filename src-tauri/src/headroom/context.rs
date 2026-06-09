use crate::session::models::ChatMessage;

use super::{compress_chat_messages, HeadroomCompressMeta};

/// Apply headroom-ai Library compress to prepared LLM messages (blocking thread).
pub fn compress_prepared_messages(
    system_parts: &[&str],
    messages: &[ChatMessage],
    model: Option<&str>,
) -> (Vec<ChatMessage>, HeadroomCompressMeta) {
    compress_chat_messages(system_parts, messages, model)
}

pub fn log_round_compress_summary(
    agent_id: &str,
    session_id: &str,
    run_id: &str,
    iteration: usize,
    reason: &str,
    meta: &HeadroomCompressMeta,
) {
    eprintln!(
        "[Agent {agent_id}] headroom round compress: session={session_id} run={run_id} iteration={iteration} reason={reason} compressed={} available={} tokens_before={:?} tokens_after={:?} tokens_saved={:?} ratio={:?} original_chars={} compressed_chars={:?} error={}",
        meta.compressed,
        meta.available,
        meta.tokens_before,
        meta.tokens_after,
        meta.tokens_saved,
        meta.compression_ratio,
        meta.original_chars,
        meta.compressed_chars,
        meta.error.as_deref().unwrap_or("none"),
    );
}
