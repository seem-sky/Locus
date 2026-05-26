use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use crate::error::AppError;
use crate::unity_bridge::{
    bind_workspace_project_path, clear_project_samples,
    lua_gc_monitor_export as bridge_lua_gc_monitor_export,
    lua_gc_monitor_get_analysis as bridge_lua_gc_monitor_get_analysis,
    lua_gc_monitor_get_samples as bridge_lua_gc_monitor_get_samples,
    lua_gc_monitor_start as bridge_lua_gc_monitor_start,
    lua_gc_monitor_status as bridge_lua_gc_monitor_status,
    lua_gc_monitor_stop as bridge_lua_gc_monitor_stop, LuaGcAnalysis, LuaGcMonitorGetSamplesRequest,
    LuaGcMonitorSamplesResponse, LuaGcMonitorStartRequest, LuaGcMonitorStatus,
};
use crate::workspace::Workspace;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorStartArgs {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub sample_interval_ms: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorSessionArgs {
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorGetSamplesArgs {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub max_points: Option<usize>,
    #[serde(default)]
    pub since_time_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorExportArgs {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorStopArgs {
    #[serde(default)]
    pub reason: Option<String>,
}

async fn workspace_project(workspace: &Workspace) -> Result<String, AppError> {
    let cwd = workspace.path.read().await.clone();
    if cwd.trim().is_empty() {
        return Err("No workspace selected.".into());
    }
    if !crate::unity_bridge::is_unity_project(&cwd) {
        return Err("Current workspace is not a Unity project.".into());
    }
    Ok(cwd)
}

#[tauri::command]
pub async fn lua_gc_monitor_start(
    workspace: State<'_, Arc<Workspace>>,
    args: LuaGcMonitorStartArgs,
) -> Result<LuaGcMonitorStatus, AppError> {
    let project_path = workspace_project(&workspace).await?;
    bind_workspace_project_path(project_path.clone()).await;
    bridge_lua_gc_monitor_start(
        &project_path,
        LuaGcMonitorStartRequest {
            session_id: args.session_id,
            sample_interval_ms: args.sample_interval_ms,
        },
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn lua_gc_monitor_stop(
    workspace: State<'_, Arc<Workspace>>,
    args: LuaGcMonitorStopArgs,
) -> Result<LuaGcMonitorStatus, AppError> {
    let project_path = workspace_project(&workspace).await?;
    bridge_lua_gc_monitor_stop(&project_path, args.reason.as_deref())
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn lua_gc_monitor_status(
    workspace: State<'_, Arc<Workspace>>,
) -> Result<LuaGcMonitorStatus, AppError> {
    let project_path = workspace_project(&workspace).await?;
    bridge_lua_gc_monitor_status(&project_path)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn lua_gc_monitor_get_samples(
    workspace: State<'_, Arc<Workspace>>,
    args: LuaGcMonitorGetSamplesArgs,
) -> Result<LuaGcMonitorSamplesResponse, AppError> {
    let project_path = workspace_project(&workspace).await?;
    bridge_lua_gc_monitor_get_samples(
        &project_path,
        LuaGcMonitorGetSamplesRequest {
            session_id: args.session_id,
            max_points: args.max_points,
            since_time_ms: args.since_time_ms,
        },
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn lua_gc_monitor_get_analysis(
    workspace: State<'_, Arc<Workspace>>,
    args: LuaGcMonitorSessionArgs,
) -> Result<LuaGcAnalysis, AppError> {
    let project_path = workspace_project(&workspace).await?;
    bridge_lua_gc_monitor_get_analysis(&project_path, args.session_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn lua_gc_monitor_export(
    workspace: State<'_, Arc<Workspace>>,
    args: LuaGcMonitorExportArgs,
) -> Result<String, AppError> {
    let project_path = workspace_project(&workspace).await?;
    bridge_lua_gc_monitor_export(
        &project_path,
        args.session_id,
        args.format.as_deref(),
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn lua_gc_monitor_clear_samples(
    workspace: State<'_, Arc<Workspace>>,
) -> Result<(), AppError> {
    let project_path = workspace_project(&workspace).await?;
    bind_workspace_project_path(project_path.clone()).await;
    clear_project_samples(&project_path).await;
    Ok(())
}
