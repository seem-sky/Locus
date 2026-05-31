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
    workingDir: params.workingDir,
    category: params.category ?? null,
    scope: params.scope ?? null,
    tags: params.tags ?? null,
    query: params.query ?? null,
    limit: params.limit ?? null,
    offset: params.offset ?? null,
  });
}

export function memoryGet(workingDir: string, scope: MemoryScope, id: string): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_get", { workingDir, scope, id });
}

export function memoryCreate(params: MemoryCreateParams): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_create", {
    workingDir: params.workingDir,
    category: params.category,
    scope: params.scope ?? null,
    content: params.content,
    tags: params.tags ?? [],
    pinned: params.pinned ?? null,
    pinWeight: params.pinWeight ?? null,
    sourceSessionId: params.sourceSessionId ?? null,
  });
}

export function memoryUpdate(params: MemoryUpdateParams): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_update", {
    workingDir: params.workingDir,
    scope: params.scope,
    id: params.id,
    category: params.category ?? null,
    content: params.content ?? null,
    tags: params.tags ?? null,
    pinned: params.pinned ?? null,
    pinWeight: params.pinWeight ?? null,
  });
}

export function memoryDelete(workingDir: string, scope: MemoryScope, id: string): Promise<void> {
  return ipcInvoke("memory_delete", { workingDir, scope, id });
}

export function memoryPin(
  workingDir: string,
  scope: MemoryScope,
  id: string,
  pinned: boolean,
  pinWeight?: number | null,
): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_pin", {
    workingDir,
    scope,
    id,
    pinned,
    pinWeight: pinWeight ?? null,
  });
}

export function memoryTagUpdate(
  workingDir: string,
  scope: MemoryScope,
  id: string,
  tags: string[],
): Promise<MemoryEntry> {
  return ipcInvoke<MemoryEntry>("memory_tag_update", { workingDir, scope, id, tags });
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
    workingDir,
    query,
    limit: options?.limit ?? null,
    tokenBudget: options?.tokenBudget ?? null,
    scopes: options?.scopes ?? null,
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
