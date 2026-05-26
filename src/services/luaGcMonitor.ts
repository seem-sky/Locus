import { ipcInvoke } from "./ipc";
import { getLocusRuntime } from "./locusRuntime";

export interface LuaGcSample {
  sessionId: string;
  frame: number;
  timeMs: number;
  runtime: string;
  memoryKb: number;
  gcDebtKb: number;
  gcStepMult: number;
  gcRunning: boolean;
  gcPhase: string;
  allocKbSinceLast: number;
  luaVersion: string;
  runtimeAvailable?: boolean;
  runtimeMessage?: string;
}

export interface LuaGcMonitorStatus {
  active: boolean;
  sessionId: string;
  sampleIntervalMs: number;
  sampleCount: number;
  runtimeAvailable: boolean;
  runtime: string;
  runtimeMessage: string;
}

export interface LuaGcMonitorSamplesResponse {
  sessionId: string;
  totalSamples: number;
  samples: LuaGcSample[];
  downsampled: boolean;
}

export interface LuaGcAlert {
  kind: string;
  severity: string;
  message: string;
  frame?: number;
  timeMs?: number;
  value?: number;
}

export interface LuaGcAnalysis {
  sessionId: string;
  sampleCount: number;
  durationMs: number;
  memoryKbMin: number;
  memoryKbMax: number;
  memoryKbLast: number;
  allocKbP95: number;
  allocKbMax: number;
  gcDebtKbMax: number;
  alerts: LuaGcAlert[];
  suggestions: string[];
}

export function luaGcMonitorStart(options?: {
  sessionId?: string;
  sampleIntervalMs?: number;
}): Promise<LuaGcMonitorStatus> {
  return ipcInvoke<LuaGcMonitorStatus>("lua_gc_monitor_start", options ?? {});
}

export function luaGcMonitorStop(reason?: string): Promise<LuaGcMonitorStatus> {
  return ipcInvoke<LuaGcMonitorStatus>("lua_gc_monitor_stop", { reason });
}

export function luaGcMonitorStatus(): Promise<LuaGcMonitorStatus> {
  return ipcInvoke<LuaGcMonitorStatus>("lua_gc_monitor_status");
}

export function luaGcMonitorGetSamples(options?: {
  sessionId?: string;
  maxPoints?: number;
  sinceTimeMs?: number;
}): Promise<LuaGcMonitorSamplesResponse> {
  return ipcInvoke<LuaGcMonitorSamplesResponse>("lua_gc_monitor_get_samples", options ?? {});
}

export function luaGcMonitorGetAnalysis(sessionId?: string): Promise<LuaGcAnalysis> {
  return ipcInvoke<LuaGcAnalysis>("lua_gc_monitor_get_analysis", { sessionId });
}

export function luaGcMonitorExport(options?: {
  sessionId?: string;
  format?: "json" | "csv";
}): Promise<string> {
  return ipcInvoke<string>("lua_gc_monitor_export", options ?? {});
}

export function luaGcMonitorClearSamples(): Promise<void> {
  return ipcInvoke<void>("lua_gc_monitor_clear_samples");
}

export function subscribeLuaGcSamples(
  handler: (sample: LuaGcSample) => void,
): Promise<() => void> {
  return getLocusRuntime().subscribe<LuaGcSample>("lua-gc-monitor-sample", handler);
}
