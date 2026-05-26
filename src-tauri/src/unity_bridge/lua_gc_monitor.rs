use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Listener};
use tokio::sync::Mutex;

use super::send_message;

const LUA_GC_SAMPLE_EVENT: &str = "lua-gc-sample";
const LUA_GC_STOPPED_EVENT: &str = "lua-gc-monitor-stopped";
const RING_CAPACITY: usize = 10_000;
const LUA_GC_REL_DIR: &str = "Library/Locus/LuaGc";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcSample {
    pub session_id: String,
    pub frame: i64,
    pub time_ms: i64,
    pub runtime: String,
    pub memory_kb: f64,
    pub gc_debt_kb: f64,
    pub gc_step_mult: i32,
    pub gc_running: bool,
    pub gc_phase: String,
    pub alloc_kb_since_last: f64,
    pub lua_version: String,
    #[serde(default)]
    pub runtime_available: bool,
    #[serde(default)]
    pub runtime_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorStatus {
    pub active: bool,
    pub session_id: String,
    pub sample_interval_ms: i32,
    pub sample_count: usize,
    pub runtime_available: bool,
    pub runtime: String,
    pub runtime_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorStartRequest {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub sample_interval_ms: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorGetSamplesRequest {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub max_points: Option<usize>,
    #[serde(default)]
    pub since_time_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcMonitorSamplesResponse {
    pub session_id: String,
    pub total_samples: usize,
    pub samples: Vec<LuaGcSample>,
    pub downsampled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcAlert {
    pub kind: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcAnalysis {
    pub session_id: String,
    pub sample_count: usize,
    pub duration_ms: i64,
    pub memory_kb_min: f64,
    pub memory_kb_max: f64,
    pub memory_kb_last: f64,
    pub alloc_kb_p95: f64,
    pub alloc_kb_max: f64,
    pub gc_debt_kb_max: f64,
    pub alerts: Vec<LuaGcAlert>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGcStoppedPayload {
    pub session_id: String,
    pub reason: String,
}

struct ProjectLuaGcState {
    active_session_id: Option<String>,
    samples: VecDeque<LuaGcSample>,
    first_time_ms: Option<i64>,
    last_time_ms: Option<i64>,
}

impl Default for ProjectLuaGcState {
    fn default() -> Self {
        Self {
            active_session_id: None,
            samples: VecDeque::new(),
            first_time_ms: None,
            last_time_ms: None,
        }
    }
}

struct LuaGcMonitorStore {
    by_project: HashMap<String, ProjectLuaGcState>,
}

impl LuaGcMonitorStore {
    fn new() -> Self {
        Self {
            by_project: HashMap::new(),
        }
    }

    fn project_state_mut(&mut self, project_path: &str) -> &mut ProjectLuaGcState {
        self.by_project
            .entry(normalize_project_key(project_path))
            .or_default()
    }
}

static STORE: OnceLock<Mutex<LuaGcMonitorStore>> = OnceLock::new();
static LISTENERS_REGISTERED: OnceLock<()> = OnceLock::new();

fn store() -> &'static Mutex<LuaGcMonitorStore> {
    STORE.get_or_init(|| Mutex::new(LuaGcMonitorStore::new()))
}

fn normalize_project_key(project_path: &str) -> String {
    let trimmed = project_path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let path = Path::new(trimmed);
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn lua_gc_root(project_path: &str) -> PathBuf {
    Path::new(project_path).join(LUA_GC_REL_DIR)
}

fn session_dir(project_path: &str, session_id: &str) -> PathBuf {
    lua_gc_root(project_path).join(session_id)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub fn register_lua_gc_monitor_listeners(app_handle: &AppHandle) {
    if LISTENERS_REGISTERED.set(()).is_err() {
        return;
    }

    let app_for_sample = app_handle.clone();
    let _ = app_handle.listen(LUA_GC_SAMPLE_EVENT, move |event| {
        let payload = event.payload();
        if let Ok(sample) = serde_json::from_str::<LuaGcSample>(payload) {
            let app = app_for_sample.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = ingest_sample(&app, sample).await {
                    eprintln!("[Locus] lua gc sample ingest error: {}", error);
                }
            });
        }
    });

    let app_for_stop = app_handle.clone();
    let _ = app_handle.listen(LUA_GC_STOPPED_EVENT, move |event| {
        let payload = event.payload();
        if let Ok(stopped) = serde_json::from_str::<LuaGcStoppedPayload>(payload) {
            let app = app_for_stop.clone();
            tauri::async_runtime::spawn(async move {
                clear_active_session(&app, &stopped.session_id).await;
            });
        }
    });
}

async fn ingest_sample(app_handle: &AppHandle, sample: LuaGcSample) -> Result<(), String> {
    let project_path = current_workspace_project_path().await;
    if project_path.is_empty() {
        return Ok(());
    }

    let session_id = sample.session_id.clone();
    {
        let mut guard = store().lock().await;
        let state = guard.project_state_mut(&project_path);
        state.active_session_id = Some(session_id.clone());
        if state.first_time_ms.is_none() {
            state.first_time_ms = Some(sample.time_ms);
        }
        state.last_time_ms = Some(sample.time_ms);
        state.samples.push_back(sample.clone());
        while state.samples.len() > RING_CAPACITY {
            state.samples.pop_front();
        }
    }

    append_sample_to_disk(&project_path, &sample)?;
    let _ = app_handle.emit("lua-gc-monitor-sample", &sample);
    Ok(())
}

async fn clear_active_session(app_handle: &AppHandle, session_id: &str) {
    let project_path = current_workspace_project_path().await;
    if project_path.is_empty() {
        return;
    }

    let mut guard = store().lock().await;
    let state = guard.project_state_mut(&project_path);
    if state
        .active_session_id
        .as_deref()
        .map(|active| active == session_id)
        .unwrap_or(false)
    {
        state.active_session_id = None;
    }
    drop(guard);

    let _ = app_handle.emit(
        "lua-gc-monitor-stopped",
        serde_json::json!({
            "sessionId": session_id,
        }),
    );
}

async fn current_workspace_project_path() -> String {
    // Workspace path is injected via monitor start/stop commands; samples use disk path from Unity.
    // When no explicit workspace binding exists, fall back to empty and skip (Unity still writes disk).
    workspace_project_path_handle()
        .lock()
        .await
        .clone()
        .unwrap_or_default()
}

static WORKSPACE_PROJECT_PATH: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn workspace_project_path_handle() -> &'static Mutex<Option<String>> {
    WORKSPACE_PROJECT_PATH.get_or_init(|| Mutex::new(None))
}

pub async fn bind_workspace_project_path(project_path: String) {
    let mut guard = workspace_project_path_handle().lock().await;
    *guard = Some(project_path);
}

pub async fn clear_project_samples(project_path: &str) {
    let key = normalize_project_key(project_path);
    let mut guard = store().lock().await;
    if let Some(state) = guard.by_project.get_mut(&key) {
        state.samples.clear();
        state.first_time_ms = None;
        state.last_time_ms = None;
        state.active_session_id = None;
    }
}

fn append_sample_to_disk(project_path: &str, sample: &LuaGcSample) -> Result<(), String> {
    let dir = session_dir(project_path, &sample.session_id);
    fs::create_dir_all(&dir).map_err(|error| format!("create session dir: {}", error))?;
    let path = dir.join("samples-rust.ndjson");
    let line = serde_json::to_string(sample)
        .map_err(|error| format!("serialize sample: {}", error))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("open {}: {}", path.display(), error))?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| format!("write {}: {}", path.display(), error))
}

pub async fn lua_gc_monitor_start(
    project_path: &str,
    request: LuaGcMonitorStartRequest,
) -> Result<LuaGcMonitorStatus, String> {
    bind_workspace_project_path(project_path.to_string()).await;

    let payload = serde_json::json!({
        "session_id": request.session_id,
        "sample_interval_ms": request.sample_interval_ms,
    });
    let resp = send_message(
        project_path,
        "lua_gc_monitor_start",
        &payload.to_string(),
    )
    .await?;
    if !resp.ok {
        return Err(resp
            .error
            .unwrap_or_else(|| "lua_gc_monitor_start failed".to_string()));
    }

    let status: LuaGcMonitorStatus = serde_json::from_str(
        resp.message
            .as_deref()
            .filter(|message| !message.trim().is_empty())
            .unwrap_or("{}"),
    )
    .map_err(|error| format!("Invalid lua_gc_monitor_start response: {}", error))?;

    {
        let mut guard = store().lock().await;
        let state = guard.project_state_mut(project_path);
        if !status.session_id.is_empty() {
            state.active_session_id = Some(status.session_id.clone());
        }
    }

    Ok(status)
}

pub async fn lua_gc_monitor_stop(project_path: &str, reason: Option<&str>) -> Result<LuaGcMonitorStatus, String> {
    bind_workspace_project_path(project_path.to_string()).await;
    let message = reason.unwrap_or("stopped");
    let resp = send_message(project_path, "lua_gc_monitor_stop", message).await?;
    if !resp.ok {
        return Err(resp
            .error
            .unwrap_or_else(|| "lua_gc_monitor_stop failed".to_string()));
    }

    serde_json::from_str(
        resp.message
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("{}"),
    )
    .map_err(|error| format!("Invalid lua_gc_monitor_stop response: {}", error))
}

pub async fn lua_gc_monitor_status(project_path: &str) -> Result<LuaGcMonitorStatus, String> {
    bind_workspace_project_path(project_path.to_string()).await;
    let resp = send_message(project_path, "lua_gc_monitor_status", "").await?;
    if !resp.ok {
        return Err(resp
            .error
            .unwrap_or_else(|| "lua_gc_monitor_status failed".to_string()));
    }

    serde_json::from_str(
        resp.message
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("{}"),
    )
    .map_err(|error| format!("Invalid lua_gc_monitor_status response: {}", error))
}

pub async fn lua_gc_monitor_get_samples(
    project_path: &str,
    request: LuaGcMonitorGetSamplesRequest,
) -> Result<LuaGcMonitorSamplesResponse, String> {
    bind_workspace_project_path(project_path.to_string()).await;

    let guard = store().lock().await;
    let key = normalize_project_key(project_path);
    let Some(state) = guard.by_project.get(&key) else {
        return Ok(LuaGcMonitorSamplesResponse {
            session_id: request.session_id.unwrap_or_default(),
            total_samples: 0,
            samples: Vec::new(),
            downsampled: false,
        });
    };

    let session_id = request
        .session_id
        .clone()
        .or(state.active_session_id.clone())
        .unwrap_or_default();

    let total_samples = state.samples.len();
    let filtered: Vec<LuaGcSample> = state
        .samples
        .iter()
        .filter(|sample| {
            if !session_id.is_empty() && sample.session_id != session_id {
                return false;
            }
            if let Some(since) = request.since_time_ms {
                if sample.time_ms < since {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    let max_points = request.max_points.unwrap_or(1200).clamp(50, RING_CAPACITY);
    let (samples, downsampled) = downsample_samples(filtered, max_points);

    Ok(LuaGcMonitorSamplesResponse {
        session_id,
        total_samples,
        samples,
        downsampled,
    })
}

fn downsample_samples(samples: Vec<LuaGcSample>, max_points: usize) -> (Vec<LuaGcSample>, bool) {
    if samples.len() <= max_points {
        return (samples, false);
    }

    let step = (samples.len() as f64 / max_points as f64).ceil() as usize;
    let mut out = Vec::with_capacity(max_points);
    let mut index = 0;
    while index < samples.len() {
        out.push(samples[index].clone());
        index = index.saturating_add(step.max(1));
    }
    (out, true)
}

pub fn analyze_samples(session_id: &str, samples: &[LuaGcSample]) -> LuaGcAnalysis {
    if samples.is_empty() {
        return LuaGcAnalysis {
            session_id: session_id.to_string(),
            sample_count: 0,
            duration_ms: 0,
            memory_kb_min: 0.0,
            memory_kb_max: 0.0,
            memory_kb_last: 0.0,
            alloc_kb_p95: 0.0,
            alloc_kb_max: 0.0,
            gc_debt_kb_max: 0.0,
            alerts: Vec::new(),
            suggestions: Vec::new(),
        };
    }

    let mut memory_values = Vec::with_capacity(samples.len());
    let mut alloc_values = Vec::with_capacity(samples.len());
    let mut gc_debt_max = 0.0_f64;

    for sample in samples {
        memory_values.push(sample.memory_kb);
        alloc_values.push(sample.alloc_kb_since_last);
        gc_debt_max = gc_debt_max.max(sample.gc_debt_kb);
    }

    memory_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    alloc_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let alloc_max = alloc_values.last().copied().unwrap_or(0.0);
    // Baseline excludes the current maximum so a single spike does not inflate P95.
    let baseline_alloc: Vec<f64> = if alloc_values.len() >= 3 {
        alloc_values
            .iter()
            .copied()
            .filter(|value| *value < alloc_max - f64::EPSILON)
            .collect()
    } else {
        alloc_values.clone()
    };
    let baseline = if baseline_alloc.is_empty() {
        &alloc_values
    } else {
        &baseline_alloc
    };
    let p95_index = ((baseline.len() as f64) * 0.95).floor() as usize;
    let alloc_p95 = baseline
        .get(p95_index.min(baseline.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0.0);

    let first_ms = samples.first().map(|sample| sample.time_ms).unwrap_or(0);
    let last_ms = samples.last().map(|sample| sample.time_ms).unwrap_or(first_ms);

    let mut alerts = Vec::new();
    let hotspot_threshold = (alloc_p95 * 2.0).max(64.0);

    for sample in samples {
        if sample.alloc_kb_since_last > hotspot_threshold {
            alerts.push(LuaGcAlert {
                kind: "gc_hotspot".to_string(),
                severity: "warn".to_string(),
                message: format!(
                    "High Lua allocation spike: {:.1} KB since last sample (frame {}).",
                    sample.alloc_kb_since_last, sample.frame
                ),
                frame: Some(sample.frame),
                time_ms: Some(sample.time_ms),
                value: Some(sample.alloc_kb_since_last),
            });
        }

    }

    // Sustained GC debt: debt above threshold for at least 3 seconds.
    const DEBT_THRESHOLD_KB: f64 = 512.0;
    const DEBT_SUSTAINED_MS: i64 = 3_000;
    let mut debt_run_start: Option<i64> = None;
    for sample in samples {
        if sample.gc_debt_kb > DEBT_THRESHOLD_KB {
            if debt_run_start.is_none() {
                debt_run_start = Some(sample.time_ms);
            } else if sample.time_ms.saturating_sub(debt_run_start.unwrap_or(sample.time_ms))
                >= DEBT_SUSTAINED_MS
            {
                alerts.push(LuaGcAlert {
                    kind: "gc_debt_sustained".to_string(),
                    severity: "warn".to_string(),
                    message: format!(
                        "GC debt stayed above {:.0} KB for at least {}s (frame {}).",
                        DEBT_THRESHOLD_KB,
                        DEBT_SUSTAINED_MS / 1000,
                        sample.frame
                    ),
                    frame: Some(sample.frame),
                    time_ms: Some(sample.time_ms),
                    value: Some(sample.gc_debt_kb),
                });
                debt_run_start = None;
            }
        } else {
            debt_run_start = None;
        }
    }

    // Atomic phase dominance.
    let atomic_count = samples
        .iter()
        .filter(|sample| sample.gc_phase.eq_ignore_ascii_case("atomic"))
        .count();
    if samples.len() >= 20 {
        let ratio = atomic_count as f64 / samples.len() as f64;
        if ratio > 0.3 {
            alerts.push(LuaGcAlert {
                kind: "gc_phase_atomic".to_string(),
                severity: "info".to_string(),
                message: format!(
                    "Atomic GC phase accounted for {:.0}% of samples; check long GC pauses.",
                    ratio * 100.0
                ),
                frame: samples.last().map(|sample| sample.frame),
                time_ms: samples.last().map(|sample| sample.time_ms),
                value: Some(ratio),
            });
        }
    }

    alerts.truncate(32);

    let mut suggestions = Vec::new();
    if alloc_max > hotspot_threshold {
        suggestions.push(
            "Allocation spikes detected: review per-frame table/string creation in Update handlers."
                .to_string(),
        );
        suggestions.push(
            "Prefer table reuse, table.concat for strings, and cached CS.* references (see gc skill)."
                .to_string(),
        );
    }

    if let (Some(first), Some(last)) = (samples.first(), samples.last()) {
        let growth = last.memory_kb - first.memory_kb;
        let duration_min = ((last.time_ms - first.time_ms).max(1) as f64) / 60_000.0;
        if growth > 1024.0 && duration_min >= 1.0 {
            let slope = growth / duration_min;
            if slope > 256.0 {
                alerts.push(LuaGcAlert {
                    kind: "leak_risk".to_string(),
                    severity: "warn".to_string(),
                    message: format!(
                        "Memory grew {:.0} KB over {:.1} min ({:.0} KB/min). Check timers, events, and closures.",
                        growth, duration_min, slope
                    ),
                    frame: Some(last.frame),
                    time_ms: Some(last.time_ms),
                    value: Some(slope),
                });
                suggestions.push(
                    "Sustained memory growth: verify event listeners and coroutines are removed on destroy."
                        .to_string(),
                );
            }
        }
    }

    if gc_debt_max > 512.0 {
        suggestions.push(
            "High GC debt: consider reducing per-frame garbage or tuning Lua GC step multiplier in xLua."
                .to_string(),
        );
    }

    if suggestions.is_empty() {
        suggestions.push("No critical Lua GC issues detected in this window.".to_string());
    }

    LuaGcAnalysis {
        session_id: session_id.to_string(),
        sample_count: samples.len(),
        duration_ms: last_ms.saturating_sub(first_ms),
        memory_kb_min: *memory_values.first().unwrap_or(&0.0),
        memory_kb_max: *memory_values.last().unwrap_or(&0.0),
        memory_kb_last: samples.last().map(|sample| sample.memory_kb).unwrap_or(0.0),
        alloc_kb_p95: alloc_p95,
        alloc_kb_max: alloc_max,
        gc_debt_kb_max: gc_debt_max,
        alerts,
        suggestions,
    }
}

pub async fn lua_gc_monitor_get_analysis(
    project_path: &str,
    session_id: Option<String>,
) -> Result<LuaGcAnalysis, String> {
    let samples_response = lua_gc_monitor_get_samples(
        project_path,
        LuaGcMonitorGetSamplesRequest {
            session_id,
            max_points: Some(RING_CAPACITY),
            since_time_ms: None,
        },
    )
    .await?;

    Ok(analyze_samples(
        &samples_response.session_id,
        &samples_response.samples,
    ))
}

pub async fn lua_gc_monitor_export(
    project_path: &str,
    session_id: Option<String>,
    format: Option<&str>,
) -> Result<String, String> {
    let samples_response = lua_gc_monitor_get_samples(
        project_path,
        LuaGcMonitorGetSamplesRequest {
            session_id: session_id.clone(),
            max_points: Some(RING_CAPACITY),
            since_time_ms: None,
        },
    )
    .await?;

    let session = if samples_response.session_id.is_empty() {
        session_id.unwrap_or_default()
    } else {
        samples_response.session_id
    };

    let dir = session_dir(project_path, &session);
    fs::create_dir_all(&dir).map_err(|error| format!("create export dir: {}", error))?;

    let format = format.unwrap_or("json").trim().to_ascii_lowercase();
    let timestamp = now_ms();
    let export_path = if format == "csv" {
        let path = dir.join(format!("export-{}.csv", timestamp));
        export_csv(&path, &samples_response.samples)?;
        path
    } else {
        let path = dir.join(format!("export-{}.json", timestamp));
        let payload = serde_json::json!({
            "exportedAtMs": timestamp,
            "sessionId": session,
            "sampleCount": samples_response.samples.len(),
            "samples": samples_response.samples,
            "analysis": analyze_samples(&session, &samples_response.samples),
        });
        fs::write(&path, serde_json::to_string_pretty(&payload).unwrap_or_default())
            .map_err(|error| format!("write {}: {}", path.display(), error))?;
        path
    };

    Ok(export_path.to_string_lossy().to_string())
}

fn export_csv(path: &Path, samples: &[LuaGcSample]) -> Result<(), String> {
    let mut file = fs::File::create(path).map_err(|error| format!("create csv: {}", error))?;
    writeln!(
        file,
        "timeMs,frame,memoryKb,gcDebtKb,allocKbSinceLast,gcPhase,gcRunning,luaVersion,runtime"
    )
    .map_err(|error| format!("write csv header: {}", error))?;
    for sample in samples {
        writeln!(
            file,
            "{},{},{:.3},{:.3},{:.3},{},{},{},{}",
            sample.time_ms,
            sample.frame,
            sample.memory_kb,
            sample.gc_debt_kb,
            sample.alloc_kb_since_last,
            sample.gc_phase,
            sample.gc_running,
            sample.lua_version,
            sample.runtime
        )
        .map_err(|error| format!("write csv row: {}", error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(memory: f64, alloc: f64, time_ms: i64, frame: i64) -> LuaGcSample {
        LuaGcSample {
            session_id: "s1".to_string(),
            frame,
            time_ms,
            runtime: "xlua".to_string(),
            memory_kb: memory,
            gc_debt_kb: 0.0,
            gc_step_mult: 0,
            gc_running: false,
            gc_phase: "incremental".to_string(),
            alloc_kb_since_last: alloc,
            lua_version: "Lua 5.4".to_string(),
            runtime_available: true,
            runtime_message: String::new(),
        }
    }

    #[test]
    fn analyze_detects_hotspot() {
        let samples = vec![
            sample(1000.0, 10.0, 0, 1),
            sample(1100.0, 200.0, 100, 2),
            sample(1120.0, 5.0, 200, 3),
        ];
        let analysis = analyze_samples("s1", &samples);
        assert!(analysis.alerts.iter().any(|alert| alert.kind == "gc_hotspot"));
    }

    #[test]
    fn analyze_detects_sustained_gc_debt() {
        let samples: Vec<LuaGcSample> = (0..40)
            .map(|index| {
                let mut sample = sample(1000.0, 1.0, index * 100, index);
                sample.gc_debt_kb = 600.0;
                sample
            })
            .collect();
        let analysis = analyze_samples("s1", &samples);
        assert!(analysis
            .alerts
            .iter()
            .any(|alert| alert.kind == "gc_debt_sustained"));
    }

    #[test]
    fn analyze_flags_high_atomic_phase_ratio() {
        let mut samples: Vec<LuaGcSample> = (0..40)
            .map(|index| sample(1000.0, 1.0, index * 100, index))
            .collect();
        for sample in samples.iter_mut().take(20) {
            sample.gc_phase = "atomic".to_string();
        }
        let analysis = analyze_samples("s1", &samples);
        assert!(analysis
            .alerts
            .iter()
            .any(|alert| alert.kind == "gc_phase_atomic"));
    }

    #[test]
    fn downsample_reduces_points() {
        let samples: Vec<LuaGcSample> = (0..500)
            .map(|index| sample(1000.0 + index as f64, 1.0, index, index))
            .collect();
        let (down, was_downsampled) = downsample_samples(samples, 100);
        assert!(was_downsampled);
        assert!(down.len() <= 100);
    }
}
