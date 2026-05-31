import type { ToolCallDisplay, ToolExecutionMeta } from "../types";
import { resolveToolCallDisplayShape } from "./toolCallBatches";

export interface RtkRewriteMeta {
  enabled: boolean;
  available: boolean;
  rewritten: boolean;
  originalCommand: string;
  executedCommand?: string;
}

export type RtkDisplayStatus = "rewritten" | "passthrough" | "unavailable" | "disabled";

export interface RtkExecutionDisplay {
  status: RtkDisplayStatus;
  originalCommand: string;
  executedCommand?: string;
}

export function isBashToolCall(
  toolCall: Pick<ToolCallDisplay, "name" | "arguments">,
): boolean {
  return resolveToolCallDisplayShape(toolCall).name === "bash";
}

export function normalizeExecutionMeta(
  source: { executionMeta?: ToolExecutionMeta; execution_meta?: ToolExecutionMeta } | null | undefined,
): ToolExecutionMeta | undefined {
  if (!source) return undefined;
  const meta = source.executionMeta ?? source.execution_meta;
  if (!meta || typeof meta !== "object") return undefined;
  return meta as ToolExecutionMeta;
}

function readRtkFields(meta: Partial<RtkRewriteMeta> & {
  original_command?: string;
  executed_command?: string;
}): Pick<RtkExecutionDisplay, "originalCommand" | "executedCommand"> {
  return {
    originalCommand: meta.originalCommand ?? meta.original_command ?? "",
    executedCommand: meta.executedCommand ?? meta.executed_command,
  };
}

function statusFromRtkMeta(meta: Partial<RtkRewriteMeta>): RtkDisplayStatus {
  if (meta.enabled !== true) return "disabled";
  if (meta.available !== true) return "unavailable";
  if (meta.rewritten === true) return "rewritten";
  return "passthrough";
}

export function parseRtkRewriteMeta(meta: unknown): RtkExecutionDisplay | null {
  if (!meta || typeof meta !== "object") return null;
  const fields = readRtkFields(meta as Partial<RtkRewriteMeta> & {
    original_command?: string;
    executed_command?: string;
  });
  return {
    status: statusFromRtkMeta(meta as Partial<RtkRewriteMeta>),
    ...fields,
  };
}

export function parseRtkExecutionMeta(
  executionMeta: ToolExecutionMeta | unknown,
): RtkExecutionDisplay | null {
  if (!executionMeta || typeof executionMeta !== "object") return null;
  const record = executionMeta as Record<string, unknown>;
  if (record.rtk) {
    return parseRtkRewriteMeta(record.rtk);
  }
  if ("enabled" in record || "available" in record || "rewritten" in record) {
    return parseRtkRewriteMeta(record);
  }
  return null;
}

export function parseRtkProgressInfo(info: string | undefined): RtkExecutionDisplay | null {
  if (!info?.trim()) return null;
  try {
    return parseRtkRewriteMeta(JSON.parse(info));
  } catch {
    return null;
  }
}

export function resolveRtkDisplayForToolCall(
  toolCall: Pick<ToolCallDisplay, "name" | "arguments" | "executionMeta" | "progress">,
): RtkExecutionDisplay | null {
  const fromMeta = parseRtkExecutionMeta(normalizeExecutionMeta(toolCall));
  if (fromMeta) return fromMeta;
  if (toolCall.progress?.state === "rtk") {
    const fromProgress = parseRtkProgressInfo(toolCall.progress.info);
    if (fromProgress) return fromProgress;
  }
  return null;
}
