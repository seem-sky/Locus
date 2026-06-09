use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::{
    compress_tool_output, enabled, execution_meta_json, min_compress_chars, library_available,
    HeadroomCompressMeta, HeadroomRewriteMeta,
};

pub fn tool_native_meta(
    _tool_name: &str,
    original_command: &str,
    executed_command: Option<&str>,
) -> HeadroomRewriteMeta {
    let original_command = original_command.to_string();
    if !enabled() {
        return HeadroomRewriteMeta {
            enabled: false,
            available: false,
            rewritten: false,
            original_command,
            executed_command: None,
        };
    }
    HeadroomRewriteMeta {
        enabled: true,
        available: library_available(),
        rewritten: false,
        original_command,
        executed_command: executed_command.map(str::to_string),
    }
}

pub fn record_execution_meta(
    sink: Option<&Arc<Mutex<Option<Value>>>>,
    rewrite: HeadroomRewriteMeta,
    compress: Option<HeadroomCompressMeta>,
) {
    let Some(sink) = sink else {
        return;
    };
    if let Ok(mut slot) = sink.lock() {
        *slot = Some(execution_meta_json(rewrite, compress));
    }
}

pub async fn maybe_compress_tool_output(
    body: String,
    model: Option<&str>,
) -> (String, Option<HeadroomCompressMeta>) {
    if !enabled() || body.chars().count() < min_compress_chars() {
        return (body, None);
    }

    let output_for_compress = body.clone();
    let model = model.map(str::to_string);
    let (compressed, meta) = tokio::task::spawn_blocking(move || {
        compress_tool_output(&output_for_compress, model.as_deref())
    })
    .await
    .unwrap_or_else(|error| {
        (
            body.clone(),
            HeadroomCompressMeta {
                enabled: enabled(),
                available: false,
                compressed: false,
                original_chars: body.chars().count(),
                compressed_chars: None,
                tokens_before: None,
                tokens_after: None,
                tokens_saved: None,
                compression_ratio: None,
                transforms_applied: Vec::new(),
                ccr_hashes: Vec::new(),
                error: Some(error.to_string()),
            },
        )
    });

    if meta.compressed {
        (compressed, Some(meta))
    } else {
        (body, Some(meta))
    }
}

pub async fn finalize_success_output(
    tool_name: &str,
    original_command: &str,
    executed_command: Option<&str>,
    model: Option<&str>,
    sink: Option<&Arc<Mutex<Option<Value>>>>,
    output: String,
) -> String {
    let rewrite_meta = tool_native_meta(tool_name, original_command, executed_command);
    let (body, compress_meta) = maybe_compress_tool_output(output, model).await;
    if let Some(ref meta) = compress_meta {
        log_tool_output_compress_summary(tool_name, meta);
    }
    record_execution_meta(sink, rewrite_meta, compress_meta);
    body
}

fn log_tool_output_compress_summary(tool_name: &str, meta: &HeadroomCompressMeta) {
    eprintln!(
        "[Headroom] tool output compress: tool={tool_name} compressed={} available={} original_chars={} compressed_chars={:?} tokens_saved={:?} error={}",
        meta.compressed,
        meta.available,
        meta.original_chars,
        meta.compressed_chars,
        meta.tokens_saved,
        meta.error.as_deref().unwrap_or("none"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_native_meta_disabled_when_headroom_off() {
        let prior = std::env::var("LOCUS_HEADROOM_DISABLED").ok();
        std::env::set_var("LOCUS_HEADROOM_DISABLED", "1");
        let meta = tool_native_meta("list", "list(path=\".\")", Some("native list"));
        assert!(!meta.enabled);
        if let Some(value) = prior {
            std::env::set_var("LOCUS_HEADROOM_DISABLED", value);
        } else {
            std::env::remove_var("LOCUS_HEADROOM_DISABLED");
        }
    }
}
