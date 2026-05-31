import { reactive } from "vue";
import { normalizeLocale } from "../i18n";
import { ipcInvoke } from "../services/ipc";

export interface AgentResponseSettings {
  /** Force assistant replies and thinking blocks to use Simplified Chinese */
  forceChineseChat: boolean;
}

const STORAGE_KEY = "locus-agent-response-settings";

const defaults: AgentResponseSettings = {
  forceChineseChat: false,
};

function syncToBackend(settings: AgentResponseSettings) {
  void ipcInvoke("set_agent_response_settings", {
    forceChineseChat: settings.forceChineseChat,
  }).catch(() => { /* backend may be unavailable during tests */ });
}

function load(): AgentResponseSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<AgentResponseSettings>;
      const settings = { ...defaults, ...parsed };
      syncToBackend(settings);
      return settings;
    }
  } catch { /* ignore */ }
  syncToBackend(defaults);
  return { ...defaults };
}

function save(settings: AgentResponseSettings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch { /* ignore */ }
  syncToBackend(settings);
}

const state = reactive<AgentResponseSettings>(load());

export function useAgentResponseSettings() {
  function set<K extends keyof AgentResponseSettings>(key: K, value: AgentResponseSettings[K]) {
    state[key] = value;
    save({ ...state });
  }

  return { state, set };
}

export function resolveChatResponseLocale(uiLocale: string): string {
  if (state.forceChineseChat) return "zh";
  return normalizeLocale(uiLocale) ?? "en";
}
