use super::{make_exec, ToolDef, ToolResult};
use crate::unity_bridge::{
    lua_gc_monitor_get_analysis, lua_gc_monitor_get_samples, lua_gc_monitor_status,
    LuaGcMonitorGetSamplesRequest,
};

pub fn lua_gc_analyze() -> ToolDef {
    let prompt = crate::prompt::parse_tool_prompt(crate::prompt::tools::LUA_GC_ANALYZE);
    ToolDef {
        name: "lua_gc_analyze".to_string(),
        description: prompt.description,
        parameters: prompt.parameters,
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let project_path = match ctx.working_dir {
                    Some(path) if !path.trim().is_empty() => path.trim().to_string(),
                    _ => {
                        return ToolResult {
                            output: "Tool 'lua_gc_analyze' requires a selected Unity project working directory."
                                .to_string(),
                            is_error: true,
                        };
                    }
                };

                if !crate::unity_bridge::is_unity_project(&project_path) {
                    return ToolResult {
                        output: "Current workspace is not a Unity project.".to_string(),
                        is_error: true,
                    };
                }

                let session_id = args
                    .get("sessionId")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);

                let max_sample_points = args
                    .get("maxSamplePoints")
                    .and_then(|value| value.as_u64())
                    .map(|value| value.clamp(10, 500) as usize)
                    .unwrap_or(50);
                let original_command = format!(
                    "lua_gc_analyze(sessionId={:?}, maxSamplePoints={max_sample_points})",
                    session_id.as_deref()
                );

                let status = match lua_gc_monitor_status(&project_path).await {
                    Ok(status) => status,
                    Err(error) => {
                        return ToolResult {
                            output: error,
                            is_error: true,
                        };
                    }
                };

                let analysis = match lua_gc_monitor_get_analysis(&project_path, session_id.clone()).await
                {
                    Ok(analysis) => analysis,
                    Err(error) => {
                        return ToolResult {
                            output: error,
                            is_error: true,
                        };
                    }
                };

                let samples = match lua_gc_monitor_get_samples(
                    &project_path,
                    LuaGcMonitorGetSamplesRequest {
                        session_id: session_id.or_else(|| {
                            if status.session_id.is_empty() {
                                None
                            } else {
                                Some(status.session_id.clone())
                            }
                        }),
                        max_points: Some(max_sample_points),
                        since_time_ms: None,
                    },
                )
                .await
                {
                    Ok(samples) => samples,
                    Err(error) => {
                        return ToolResult {
                            output: error,
                            is_error: true,
                        };
                    }
                };

                let tail: Vec<_> = samples
                    .samples
                    .iter()
                    .rev()
                    .take(8)
                    .map(|sample| {
                        serde_json::json!({
                            "frame": sample.frame,
                            "timeMs": sample.time_ms,
                            "memoryKb": sample.memory_kb,
                            "gcDebtKb": sample.gc_debt_kb,
                            "allocKbSinceLast": sample.alloc_kb_since_last,
                            "gcPhase": sample.gc_phase,
                        })
                    })
                    .collect();

                let payload = serde_json::json!({
                    "status": status,
                    "analysis": analysis,
                    "recentSamples": tail.into_iter().rev().collect::<Vec<_>>(),
                    "sampleSummary": {
                        "returned": samples.samples.len(),
                        "total": samples.total_samples,
                        "downsampled": samples.downsampled,
                    },
                    "hint": if !status.runtime_available {
                        "xLua runtime not registered. Start Play Mode and register LuaEnv via LocusBridge, or run the bootstrap snippet documented in docs/lua-gc-monitor.md."
                    } else if !status.active && analysis.sample_count == 0 {
                        "No Lua GC samples yet. Ask the user to start recording from the Lua GC monitor panel, or call lua_gc_monitor_start via IPC during Play Mode."
                    } else {
                        "Combine rule alerts with knowledge/skill/gc.md patterns when recommending fixes."
                    },
                });

                let output = serde_json::to_string_pretty(&payload).unwrap_or_else(|error| {
                    format!("Failed to serialize lua_gc_analyze response: {}", error)
                });
                let rewrite_meta = crate::headroom::tool_native_meta(
                    "lua_gc_analyze",
                    &original_command,
                    Some("Unity Lua GC monitor"),
                );
                let (body, compress_meta) = crate::headroom::maybe_compress_tool_output(
                    output,
                    ctx.llm_model.as_deref(),
                )
                .await;
                crate::headroom::record_execution_meta(
                    ctx.execution_meta_sink.as_ref(),
                    rewrite_meta,
                    compress_meta,
                );
                ToolResult {
                    output: body,
                    is_error: false,
                }
            })
        }),
    }
}
