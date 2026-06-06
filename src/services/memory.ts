import { ipcInvoke } from "./ipc";
import { waitForTauriWindowRuntime } from "./tauriRuntime";
import type {
  MemoryCategory,
  MemoryEntry,
  MemoryRetrieveHit,
  MemoryScope,
} from "../types";

export interface MemoryListParams {
  workingDir: string;
  category?: MemoryCategory | null;
  scope?: MemoryScope | null;
  tags?: string[] | null;
  query?: string | null;
  limit?: number | null;
  offset?: number | null;
}

export interface MemoryCreateParams {
  workingDir: string;
  category: MemoryCategory;
  scope?: MemoryScope | null;
  content: string;
  tags?: string[];
  pinned?: boolean;
  pinWeight?: number;
  sourceSessionId?: string | null;
}

export interface MemoryUpdateParams {
  workingDir: string;
  scope: MemoryScope;
  id: string;
  category?: MemoryCategory;
  content?: string;
  tags?: string[];
  pinned?: boolean;
  pinWeight?: number;
}

export function memoryList(params: MemoryListParams): Promise<MemoryEntry[]> {
  return ipcInvoke<MemoryEntry[]>("memory_list", {
    request: {
      workingDir: params.workingDir,
      category: params.category ?? null,
      scope: params.scope ?? null,
      tags: params.tags ?? null,
      query: params.query ?? null,
      limit: params.limit ?? null,
      offset: params.offset ?? null,
    },
  });
}

export function memoryGet(workingDir: string, scope: MemoryScope, id: string): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_get", {
    request: { workingDir, scope, id },
  });
}

export function memoryCreate(params: MemoryCreateParams): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_create", {
    request: {
      workingDir: params.workingDir,
      category: params.category,
      scope: params.scope ?? null,
      content: params.content,
      tags: params.tags ?? [],
      pinned: params.pinned ?? null,
      pinWeight: params.pinWeight ?? null,
      sourceSessionId: params.sourceSessionId ?? null,
    },
  });
}

export function memoryUpdate(params: MemoryUpdateParams): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_update", {
    request: {
      workingDir: params.workingDir,
      scope: params.scope,
      id: params.id,
      category: params.category ?? null,
      content: params.content ?? null,
      tags: params.tags ?? null,
      pinned: params.pinned ?? null,
      pinWeight: params.pinWeight ?? null,
    },
  });
}

export function memoryDelete(workingDir: string, scope: MemoryScope, id: string): Promise<void> {
  return ipcInvoke("memory_delete", {
    request: { workingDir, scope, id },
  });
}

export function memoryPin(
  workingDir: string,
  scope: MemoryScope,
  id: string,
  pinned: boolean,
  pinWeight?: number | null,
): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_pin", {
    request: {
      workingDir,
      scope,
      id,
      pinned,
      pinWeight: pinWeight ?? null,
    },
  });
}

export function memoryTagUpdate(
  workingDir: string,
  scope: MemoryScope,
  id: string,
  tags: string[],
): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_tag_update", {
    request: { workingDir, scope, id, tags },
  });
}

export function memoryRetrieve(
  workingDir: string,
  query: string,
  options?: {
    limit?: number;
    tokenBudget?: number;
    scopes?: MemoryScope[];
  },
): Promise<MemoryRetrieveHit[]> {
  return ipcInvoke<MemoryRetrieveHit[]>("memory_retrieve", {
    request: {
      workingDir,
      query,
      limit: options?.limit ?? null,
      tokenBudget: options?.tokenBudget ?? null,
      scopes: options?.scopes ?? null,
    },
  });
}

export function staleMemoryProposals(sessionId: string): Promise<void> {
  return ipcInvoke("stale_memory_proposals", { sessionId });
}

export function ignoreMemoryProposal(sessionId: string, proposalId: string): Promise<void> {
  return ipcInvoke("ignore_memory_proposal", { sessionId, proposalId });
}

export function applyMemoryProposal(
  sessionId: string,
  proposalId: string,
): Promise<void> {
  return ipcInvoke("apply_memory_proposal", { sessionId, proposalId });
}

export interface AgentMemoryStatus {
  available: boolean;
  status: string;
  version?: string | null;
  viewerPort?: number | null;
  baseUrl: string;
  autostartEnabled: boolean;
  bundleVersion?: string | null;
  usingBundledRuntime?: boolean;
  error?: string | null;
  llmConfigured?: boolean;
  llmProvider?: string | null;
  llmWarning?: string | null;
}

function isTransientIpcError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("Failed to fetch") || message.includes("Load failed");
}

async function invokeAgentmemoryCommand<T>(command: string): Promise<T> {
  if (!(await waitForTauriWindowRuntime())) {
    throw new Error("Agentmemory service is only available inside the Locus desktop app.");
  }

  const delays = [0, 250, 750];
  let lastError: unknown;
  for (const delay of delays) {
    if (delay > 0) {
      await new Promise((resolve) => setTimeout(resolve, delay));
    }
    try {
      return await ipcInvoke<T>(command);
    } catch (error) {
      lastError = error;
      if (!isTransientIpcError(error)) {
        break;
      }
    }
  }
  throw lastError;
}

export function agentmemoryStatus(): Promise<AgentMemoryStatus> {
  return invokeAgentmemoryCommand<AgentMemoryStatus>("agentmemory_status");
}

export function agentmemoryStart(): Promise<AgentMemoryStatus> {
  return invokeAgentmemoryCommand<AgentMemoryStatus>("agentmemory_start");
}

export function agentmemoryStop(): Promise<AgentMemoryStatus> {
  return invokeAgentmemoryCommand<AgentMemoryStatus>("agentmemory_stop");
}

export interface AgentMemoryAction {
  id: string;
  title: string;
  description?: string | null;
  status: string;
  priority?: number | null;
  project?: string | null;
  createdBy?: string | null;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export function agentmemoryActionList(
  workingDir: string,
  status?: string | null,
): Promise<AgentMemoryAction[]> {
  return ipcInvoke<AgentMemoryAction[]>("agentmemory_action_list", {
    request: { workingDir, status: status ?? null },
  });
}

export function agentmemoryActionCreate(params: {
  workingDir: string;
  title: string;
  description?: string | null;
  priority?: number | null;
  tags?: string[];
  parentId?: string | null;
}): Promise<AgentMemoryAction> {
  return ipcInvoke<AgentMemoryAction>("agentmemory_action_create", {
    request: {
      workingDir: params.workingDir,
      title: params.title,
      description: params.description ?? null,
      priority: params.priority ?? null,
      tags: params.tags ?? [],
      parentId: params.parentId ?? null,
    },
  });
}

export function agentmemoryActionUpdate(params: {
  workingDir: string;
  actionId: string;
  status?: string | null;
  title?: string | null;
  description?: string | null;
  priority?: number | null;
  result?: string | null;
}): Promise<AgentMemoryAction> {
  return ipcInvoke<AgentMemoryAction>("agentmemory_action_update", {
    request: {
      workingDir: params.workingDir,
      actionId: params.actionId,
      status: params.status ?? null,
      title: params.title ?? null,
      description: params.description ?? null,
      priority: params.priority ?? null,
      result: params.result ?? null,
    },
  });
}

export interface AgentMemorySessionRow {
  id: string;
  title?: string | null;
  status?: string | null;
  project?: string | null;
  observationCount?: number | null;
  startedAt?: string | null;
  endedAt?: string | null;
}

export type AgentMemoryFeatureFlag = {
  key: string;
  label?: string;
  enabled?: boolean;
  needsLlm?: boolean;
  description?: string;
};

export interface AgentMemoryInsights {
  sessions: unknown;
  profile?: unknown;
  patterns?: unknown;
  graphStats?: unknown;
  featureFlags?: AgentMemoryFeatureFlag[];
  errors: string[];
}

function parseSessionRows(payload: unknown): AgentMemorySessionRow[] {
  if (!payload || typeof payload !== "object") return [];
  const record = payload as Record<string, unknown>;
  const list = Array.isArray(record.sessions)
    ? record.sessions
    : Array.isArray(payload)
      ? payload
      : [];
  return list
    .filter((item): item is Record<string, unknown> => !!item && typeof item === "object")
    .map((item) => ({
      id: String(item.id ?? item.sessionId ?? ""),
      title: typeof item.title === "string" ? item.title : null,
      status: typeof item.status === "string" ? item.status : null,
      project: typeof item.project === "string" ? item.project : null,
      observationCount:
        typeof item.observationCount === "number"
          ? item.observationCount
          : typeof item.observation_count === "number"
            ? item.observation_count
            : null,
      startedAt:
        typeof item.startedAt === "string"
          ? item.startedAt
          : typeof item.started_at === "string"
            ? item.started_at
            : null,
      endedAt:
        typeof item.endedAt === "string"
          ? item.endedAt
          : typeof item.ended_at === "string"
            ? item.ended_at
            : null,
    }))
    .filter((row) => row.id.length > 0);
}

export function agentmemoryInsights(workingDir: string): Promise<AgentMemoryInsights> {
  return ipcInvoke<{
    sessions: unknown;
    profile?: unknown;
    patterns?: unknown;
    graph_stats?: unknown;
    feature_flags?: unknown;
    errors: string[];
  }>("agentmemory_insights", {
    request: { workingDir },
  }).then((response) => ({
    sessions: response.sessions,
    profile: response.profile,
    patterns: response.patterns,
    graphStats: response.graph_stats,
    featureFlags: parseFeatureFlags(response.feature_flags),
    errors: response.errors ?? [],
  }));
}

function parseFeatureFlags(raw: unknown): AgentMemoryFeatureFlag[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((item): item is Record<string, unknown> => !!item && typeof item === "object")
    .map((item) => ({
      key: String(item.key ?? ""),
      label: typeof item.label === "string" ? item.label : undefined,
      enabled: typeof item.enabled === "boolean" ? item.enabled : undefined,
      needsLlm: typeof item.needsLlm === "boolean" ? item.needsLlm : undefined,
      description: typeof item.description === "string" ? item.description : undefined,
    }))
    .filter((item) => item.key.length > 0);
}

export { parseSessionRows };

export function isAgentMemoryPatternNoise(text: string): boolean {
  const lower = text.trim().toLowerCase();
  if (!lower) return true;
  const markers = [
    "posttoolusefailure",
    "post_tool_failure",
    "post tool use failure",
    "hook triggered",
    "hook fired",
    "recurring error:",
    "with no details",
    "pre-tool-use hook",
    "pre_tool_use hook",
  ];
  return markers.some((marker) => lower.includes(marker));
}

export function agentmemoryConsolidate(params?: {
  tier?: string | null;
  force?: boolean | null;
}): Promise<unknown> {
  return ipcInvoke("agentmemory_consolidate", {
    request: {
      tier: params?.tier ?? null,
      force: params?.force ?? null,
    },
  });
}
