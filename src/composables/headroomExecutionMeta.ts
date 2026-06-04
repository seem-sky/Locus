import type { ToolCallDisplay, ToolExecutionMeta } from "../types";
import { resolveToolCallDisplayShape } from "./toolCallBatches";

export interface HeadroomRewriteMeta {
  enabled: boolean;
  available: boolean;
  rewritten: boolean;
  originalCommand: string;
  executedCommand?: string;
}

export interface HeadroomCompressMeta {
  enabled: boolean;
  available: boolean;
  compressed: boolean;
  originalChars: number;
  compressedChars?: number;
  tokensBefore?: number;
  tokensAfter?: number;
  tokensSaved?: number;
  compressionRatio?: number;
  transformsApplied?: string[];
  ccrHashes?: string[];
  error?: string;
}

export type HeadroomRewriteStatus = "rewritten" | "passthrough" | "unavailable" | "disabled";

export interface HeadroomExecutionDisplay {
  rewriteStatus: HeadroomRewriteStatus;
  originalCommand: string;
  executedCommand?: string;
  compress?: HeadroomCompressMeta | null;
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

function readRewriteFields(meta: Partial<HeadroomRewriteMeta> & {
  original_command?: string;
  executed_command?: string;
}): Pick<HeadroomExecutionDisplay, "originalCommand" | "executedCommand"> {
  return {
    originalCommand: meta.originalCommand ?? meta.original_command ?? "",
    executedCommand: meta.executedCommand ?? meta.executed_command,
  };
}

function rewriteStatusFromMeta(meta: Partial<HeadroomRewriteMeta>): HeadroomRewriteStatus {
  if (meta.enabled !== true) return "disabled";
  if (meta.available !== true) return "unavailable";
  if (meta.rewritten === true) return "rewritten";
  return "passthrough";
}

function readCompressMeta(meta: unknown): HeadroomCompressMeta | null {
  if (!meta || typeof meta !== "object") return null;
  const record = meta as Record<string, unknown>;
  return {
    enabled: record.enabled === true,
    available: record.available === true,
    compressed: record.compressed === true,
    originalChars: Number(record.originalChars ?? record.original_chars ?? 0),
    compressedChars: record.compressedChars != null
      ? Number(record.compressedChars)
      : record.compressed_chars != null
        ? Number(record.compressed_chars)
        : undefined,
    tokensBefore: record.tokensBefore != null
      ? Number(record.tokensBefore)
      : record.tokens_before != null
        ? Number(record.tokens_before)
        : undefined,
    tokensAfter: record.tokensAfter != null
      ? Number(record.tokensAfter)
      : record.tokens_after != null
        ? Number(record.tokens_after)
        : undefined,
    tokensSaved: record.tokensSaved != null
      ? Number(record.tokensSaved)
      : record.tokens_saved != null
        ? Number(record.tokens_saved)
        : undefined,
    compressionRatio: record.compressionRatio != null
      ? Number(record.compressionRatio)
      : record.compression_ratio != null
        ? Number(record.compression_ratio)
        : undefined,
    transformsApplied: Array.isArray(record.transformsApplied)
      ? record.transformsApplied.map(String)
      : Array.isArray(record.transforms_applied)
        ? record.transforms_applied.map(String)
        : undefined,
    ccrHashes: Array.isArray(record.ccrHashes)
      ? record.ccrHashes.map(String)
      : Array.isArray(record.ccr_hashes)
        ? record.ccr_hashes.map(String)
        : undefined,
    error: typeof record.error === "string" ? record.error : undefined,
  };
}

export function parseHeadroomRewriteMeta(meta: unknown): HeadroomExecutionDisplay | null {
  if (!meta || typeof meta !== "object") return null;
  const fields = readRewriteFields(meta as Partial<HeadroomRewriteMeta> & {
    original_command?: string;
    executed_command?: string;
  });
  return {
    rewriteStatus: rewriteStatusFromMeta(meta as Partial<HeadroomRewriteMeta>),
    ...fields,
    compress: null,
  };
}

export function parseHeadroomExecutionMeta(
  executionMeta: ToolExecutionMeta | unknown,
): HeadroomExecutionDisplay | null {
  if (!executionMeta || typeof executionMeta !== "object") return null;
  const record = executionMeta as Record<string, unknown>;

  if (record.headroom && typeof record.headroom === "object") {
    const headroom = record.headroom as Record<string, unknown>;
    const rewrite = parseHeadroomRewriteMeta(headroom.rewrite);
    if (!rewrite) return null;
    return {
      ...rewrite,
      compress: readCompressMeta(headroom.compress),
    };
  }

  // Legacy RTK meta shape (rewrite-only progress payloads)
  if (record.rtk) {
    return parseHeadroomRewriteMeta(record.rtk);
  }
  if ("enabled" in record || "available" in record || "rewritten" in record) {
    return parseHeadroomRewriteMeta(record);
  }
  return null;
}

export function parseHeadroomProgressInfo(info: string | undefined): HeadroomExecutionDisplay | null {
  if (!info?.trim()) return null;
  try {
    const parsed = JSON.parse(info) as unknown;
    if (parsed && typeof parsed === "object") {
      const record = parsed as Record<string, unknown>;
      if (record.rewrite || record.compress) {
        const rewrite = parseHeadroomRewriteMeta(record.rewrite ?? record);
        if (!rewrite) return null;
        return {
          ...rewrite,
          compress: readCompressMeta(record.compress),
        };
      }
    }
    return parseHeadroomRewriteMeta(parsed);
  } catch {
    return null;
  }
}

export function resolveHeadroomDisplayForToolCall(
  toolCall: Pick<ToolCallDisplay, "name" | "arguments" | "executionMeta" | "progress">,
): HeadroomExecutionDisplay | null {
  const fromMeta = parseHeadroomExecutionMeta(normalizeExecutionMeta(toolCall));
  if (fromMeta) return fromMeta;
  if (toolCall.progress?.state === "headroom" || toolCall.progress?.state === "rtk") {
    const fromProgress = parseHeadroomProgressInfo(toolCall.progress.info);
    if (fromProgress) return fromProgress;
  }
  return null;
}

/** @deprecated Use resolveHeadroomDisplayForToolCall */
export const resolveRtkDisplayForToolCall = resolveHeadroomDisplayForToolCall;
