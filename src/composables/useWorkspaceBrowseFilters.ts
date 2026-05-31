import { computed, reactive, ref, watch } from "vue";
import {
  getWorkspaceBrowseFilters,
  setWorkspaceBrowseFilters,
} from "../services/workspaceConfig";
import { useProjectStore } from "../stores/project";

export interface WorkspaceBrowseFilters {
  blockedFolderNames: string[];
  blockedFileNames: string[];
  blockedExtensions: string[];
}

const LEGACY_STORAGE_KEY = "locus-workspace-browse-filters";

const defaults: WorkspaceBrowseFilters = {
  blockedFolderNames: [],
  blockedFileNames: [],
  blockedExtensions: [],
};

function splitRuleInput(raw: string): string[] {
  return raw
    .split(/[\n,;]+/)
    .map((part) => part.trim())
    .filter(Boolean);
}

function normalizeNameRules(values: string[]): string[] {
  const out: string[] = [];
  for (const value of values) {
    const normalized = value.replace(/\\/g, "/").trim();
    if (!normalized) continue;
    if (out.some((existing) => existing.toLowerCase() === normalized.toLowerCase())) {
      continue;
    }
    out.push(normalized);
  }
  return out;
}

function normalizeFolderRules(values: string[]): string[] {
  return normalizeNameRules(values)
    .map((rule) => rule.replace(/^\/+|\/+$/g, ""))
    .filter(Boolean);
}

function normalizeExtensionRules(values: string[]): string[] {
  const out: string[] = [];
  for (const value of values) {
    const trimmed = value.trim().toLowerCase();
    if (!trimmed) continue;
    const normalized = trimmed.startsWith(".") ? trimmed : `.${trimmed}`;
    if (!out.includes(normalized)) {
      out.push(normalized);
    }
  }
  return out;
}

export function normalizeWorkspaceBrowseFilters(
  input: Partial<WorkspaceBrowseFilters> | null | undefined,
): WorkspaceBrowseFilters {
  return {
    blockedFolderNames: normalizeFolderRules(input?.blockedFolderNames ?? []),
    blockedFileNames: normalizeNameRules(input?.blockedFileNames ?? []),
    blockedExtensions: normalizeExtensionRules(input?.blockedExtensions ?? []),
  };
}

export function workspaceBrowseFiltersActive(
  filters: WorkspaceBrowseFilters,
): boolean {
  return filters.blockedFolderNames.length > 0
    || filters.blockedFileNames.length > 0
    || filters.blockedExtensions.length > 0;
}

function applyToState(filters: WorkspaceBrowseFilters) {
  state.blockedFolderNames = filters.blockedFolderNames;
  state.blockedFileNames = filters.blockedFileNames;
  state.blockedExtensions = filters.blockedExtensions;
}

function loadLegacyLocalStorageFilters(): WorkspaceBrowseFilters | null {
  try {
    const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return null;
    return normalizeWorkspaceBrowseFilters(JSON.parse(raw) as Partial<WorkspaceBrowseFilters>);
  } catch {
    return null;
  }
}

function clearLegacyLocalStorageFilters() {
  try {
    localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

const state = reactive<WorkspaceBrowseFilters>({ ...defaults });
const revision = ref(0);
const loading = ref(false);
let loadToken = 0;
let workspaceWatchStarted = false;

async function reloadFromWorkspace(workingDir: string) {
  const token = ++loadToken;
  loading.value = true;

  if (!workingDir.trim()) {
    applyToState({ ...defaults });
    revision.value += 1;
    loading.value = false;
    return;
  }

  try {
    let filters = normalizeWorkspaceBrowseFilters(await getWorkspaceBrowseFilters());
    if (token !== loadToken) return;

    const legacy = loadLegacyLocalStorageFilters();
    if (legacy && workspaceBrowseFiltersActive(legacy) && !workspaceBrowseFiltersActive(filters)) {
      filters = legacy;
      await setWorkspaceBrowseFilters(filters);
      if (token !== loadToken) return;
      clearLegacyLocalStorageFilters();
    }

    applyToState(filters);
    revision.value += 1;
  } catch {
    if (token === loadToken) {
      applyToState({ ...defaults });
      revision.value += 1;
    }
  } finally {
    if (token === loadToken) {
      loading.value = false;
    }
  }
}

async function persist(filters: WorkspaceBrowseFilters) {
  const project = useProjectStore();
  if (!project.workingDir.trim()) return;
  await setWorkspaceBrowseFilters(filters);
}

function ensureWorkspaceWatch() {
  if (workspaceWatchStarted) return;
  workspaceWatchStarted = true;
  const project = useProjectStore();
  watch(
    () => project.workingDir,
    (workingDir) => {
      void reloadFromWorkspace(workingDir);
    },
    { immediate: true },
  );
}

export function useWorkspaceBrowseFilters() {
  ensureWorkspaceWatch();

  const payload = computed(() => ({ ...state }));

  async function replace(next: WorkspaceBrowseFilters) {
    const normalized = normalizeWorkspaceBrowseFilters(next);
    applyToState(normalized);
    revision.value += 1;
    try {
      await persist(normalized);
    } catch {
      /* keep in-memory state; user can retry after opening a workspace */
    }
  }

  function setList<K extends keyof WorkspaceBrowseFilters>(
    key: K,
    values: WorkspaceBrowseFilters[K],
  ) {
    void replace({ ...state, [key]: values });
  }

  function addRule<K extends keyof WorkspaceBrowseFilters>(
    key: K,
    raw: string,
  ) {
    const additions = splitRuleInput(raw);
    if (!additions.length) return;
    void replace({ ...state, [key]: [...state[key], ...additions] } as WorkspaceBrowseFilters);
  }

  function removeRule<K extends keyof WorkspaceBrowseFilters>(
    key: K,
    value: string,
  ) {
    void replace({
      ...state,
      [key]: state[key].filter((entry) => entry !== value),
    } as WorkspaceBrowseFilters);
  }

  function reset() {
    void replace({ ...defaults });
  }

  return {
    state,
    payload,
    revision,
    loading,
    replace,
    setList,
    addRule,
    removeRule,
    reset,
    splitRuleInput,
    reloadFromWorkspace,
  };
}
