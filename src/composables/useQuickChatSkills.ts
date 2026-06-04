import { computed, ref, type Ref } from "vue";
import type { CommandDef } from "./chatInputIntents";
import { useCommandRegistry } from "./useCommandRegistry";
import type { KnowledgeDocument, SkillIntentItem } from "../types";
import type { SkillManifest } from "../types";

export const QUICK_CHAT_SKILLS_STORAGE_KEY = "locus:quickChatSkillPins";
export const QUICK_CHAT_SKILLS_CHANGED_EVENT = "locus-quick-chat-skills-changed";
export const MAX_QUICK_CHAT_SKILLS = 6;

/** Default one-click skills for this workspace (matched by skill `dirName`). */
export const DEFAULT_QUICK_CHAT_SKILL_DIR_NAMES = [
  "systematic-debugging",
  "writing-plans",
  "verification-before-completion",
  "executing-plans",
  "code-reviewer",
] as const;

export interface QuickChatSkillPin {
  source: SkillIntentItem["source"];
  dirName: string;
}

export const quickChatPinsRevision = ref(0);

export function skillPinKey(pin: QuickChatSkillPin): string {
  return `${pin.source}:${pin.dirName}`;
}

function commandSkillKey(command: CommandDef): string | null {
  if (!command.skill) return null;
  return `${command.skill.source}:${command.skill.dirName}`;
}

export function notifyQuickChatSkillsChanged() {
  quickChatPinsRevision.value += 1;
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(QUICK_CHAT_SKILLS_CHANGED_EVENT));
  }
}

export function loadQuickChatSkillPins(): QuickChatSkillPin[] | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(QUICK_CHAT_SKILLS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as QuickChatSkillPin[];
    if (!Array.isArray(parsed)) return null;
    return parsed
      .filter((item) =>
        item
        && (item.source === "app" || item.source === "project" || item.source === "pluginApp" || item.source === "pluginProject")
        && typeof item.dirName === "string"
        && item.dirName.trim(),
      )
      .map((item) => ({
        source: item.source,
        dirName: item.dirName.trim(),
      }))
      .slice(0, MAX_QUICK_CHAT_SKILLS);
  } catch {
    return null;
  }
}

export function saveQuickChatSkillPins(pins: QuickChatSkillPin[]) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(
      QUICK_CHAT_SKILLS_STORAGE_KEY,
      JSON.stringify(pins.slice(0, MAX_QUICK_CHAT_SKILLS)),
    );
    notifyQuickChatSkillsChanged();
  } catch {
    // ignore persistence failures
  }
}

export function materializeDefaultQuickChatPins(skills: SkillManifest[]): QuickChatSkillPin[] {
  const ordered: QuickChatSkillPin[] = [];
  const used = new Set<string>();
  for (const dirName of DEFAULT_QUICK_CHAT_SKILL_DIR_NAMES) {
    const skill = skills.find((item) => item.dirName.toLowerCase() === dirName.toLowerCase());
    if (!skill) continue;
    const key = skillPinKey({ source: skill.source, dirName: skill.dirName });
    if (used.has(key)) continue;
    ordered.push({ source: skill.source, dirName: skill.dirName });
    used.add(key);
  }
  return ordered;
}

export function resolveCurrentQuickChatPins(skills: SkillManifest[]): QuickChatSkillPin[] {
  const stored = loadQuickChatSkillPins();
  if (stored !== null) return stored;
  return materializeDefaultQuickChatPins(skills);
}

export function isQuickChatSkillPinned(
  pin: QuickChatSkillPin,
  skills: SkillManifest[] = [],
): boolean {
  const normalizedPin = normalizeQuickChatSkillPin(pin, skills);
  return resolveCurrentQuickChatPins(skills).some(
    (item) => skillPinKey(item) === skillPinKey(normalizedPin),
  );
}

export function pinQuickChatSkill(
  pin: QuickChatSkillPin,
  skills: SkillManifest[],
): { ok: boolean; limited: boolean } {
  const normalizedPin = normalizeQuickChatSkillPin(pin, skills);
  const pins = resolveCurrentQuickChatPins(skills);
  if (pins.some((item) => skillPinKey(item) === skillPinKey(normalizedPin))) {
    return { ok: true, limited: false };
  }
  if (pins.length >= MAX_QUICK_CHAT_SKILLS) {
    return { ok: false, limited: true };
  }
  saveQuickChatSkillPins([...pins, normalizedPin]);
  return { ok: true, limited: false };
}

export function pinsFromQuickChatCommands(commands: CommandDef[]): QuickChatSkillPin[] {
  const pins: QuickChatSkillPin[] = [];
  const used = new Set<string>();
  for (const command of commands) {
    const pin = quickChatPinFromCommand(command);
    if (!pin) continue;
    const key = skillPinKey(pin);
    if (used.has(key)) continue;
    pins.push(pin);
    used.add(key);
  }
  return pins;
}

function resolveQuickChatPinsForUnpin(
  skills: SkillManifest[],
  visibleCommands: CommandDef[],
): QuickChatSkillPin[] {
  if (loadQuickChatSkillPins() !== null) {
    return resolveCurrentQuickChatPins(skills);
  }
  if (visibleCommands.length > 0) {
    return pinsFromQuickChatCommands(visibleCommands);
  }
  return materializeDefaultQuickChatPins(skills);
}

export function unpinQuickChatSkill(
  pin: QuickChatSkillPin,
  skills: SkillManifest[],
  visibleCommands: CommandDef[] = [],
) {
  const normalizedPin = normalizeQuickChatSkillPin(pin, skills);
  const pins = resolveQuickChatPinsForUnpin(skills, visibleCommands).filter(
    (item) => skillPinKey(item) !== skillPinKey(normalizedPin),
  );
  saveQuickChatSkillPins(pins);
}

export function resolveSkillPinFromDocument(
  document: Pick<KnowledgeDocument, "type" | "path" | "storageSource"> | null | undefined,
  skills: SkillManifest[],
): QuickChatSkillPin | null {
  if (!document || document.type !== "skill") return null;

  const normalizedPath = normalizeSkillDocumentPath(document.path);
  for (const skill of skills) {
    const rel = skill.relPath.trim().replace(/\\/g, "/");
    if (rel === normalizedPath) {
      return { source: skill.source, dirName: skill.dirName };
    }
  }

  for (const skill of skills) {
    const rel = skill.relPath.trim().replace(/\\/g, "/");
    const firstSegment = normalizedPath.split("/").filter(Boolean)[0] ?? "";
    if (
      normalizedPath === `${skill.dirName}.md`
      || normalizedPath === `${skill.dirName}/skill.md`
      || normalizedPath.startsWith(`${skill.dirName}/`)
      || firstSegment === skill.dirName
      || rel.endsWith(`/${normalizedPath}`)
    ) {
      return { source: skill.source, dirName: skill.dirName };
    }
  }

  const segments = normalizedPath.split("/").filter(Boolean);
  const leaf = segments[segments.length - 1] ?? "";
  const dirName = leaf.toLowerCase() === "skill.md" && segments.length > 1
    ? segments[segments.length - 2] ?? ""
    : leaf.replace(/\.md$/i, "");
  if (!dirName) return null;

  const matched = skills.find((skill) => skill.dirName.toLowerCase() === dirName.toLowerCase());
  if (matched) {
    return { source: matched.source, dirName: matched.dirName };
  }

  return {
    source: document.storageSource === "app" ? "app" : "project",
    dirName,
  };
}

export function resolveSkillPinFromManifest(
  manifest: Pick<SkillManifest, "source" | "dirName"> | null | undefined,
): QuickChatSkillPin | null {
  if (!manifest?.dirName?.trim()) return null;
  return {
    source: manifest.source,
    dirName: manifest.dirName.trim(),
  };
}

export function quickChatPinFromCommand(command: CommandDef): QuickChatSkillPin | null {
  if (!command.skill) return null;
  return {
    source: command.skill.source,
    dirName: command.skill.dirName,
  };
}

export function normalizeQuickChatSkillPin(
  pin: QuickChatSkillPin,
  skills: SkillManifest[],
): QuickChatSkillPin {
  const matched = skills.find(
    (skill) => skill.dirName.toLowerCase() === pin.dirName.toLowerCase(),
  );
  if (matched) {
    return { source: matched.source, dirName: matched.dirName };
  }
  return pin;
}

function normalizeSkillDocumentPath(path: string): string {
  const normalized = path.trim().replace(/\\/g, "/").replace(/^\/+/, "");
  if (normalized.toLowerCase().startsWith("skill/")) {
    return normalized.slice("skill/".length);
  }
  return normalized;
}

function resolveOrderedPins(skillCommands: CommandDef[]): QuickChatSkillPin[] {
  const stored = loadQuickChatSkillPins();
  if (stored !== null) return stored;

  const byDir = new Map<string, QuickChatSkillPin>();
  for (const command of skillCommands) {
    if (!command.skill) continue;
    const dirKey = command.skill.dirName.toLowerCase();
    if (!byDir.has(dirKey)) {
      byDir.set(dirKey, {
        source: command.skill.source,
        dirName: command.skill.dirName,
      });
    }
  }

  const ordered: QuickChatSkillPin[] = [];
  for (const dirName of DEFAULT_QUICK_CHAT_SKILL_DIR_NAMES) {
    const pin = byDir.get(dirName.toLowerCase());
    if (pin) ordered.push(pin);
  }
  return ordered;
}

export function resolveQuickChatSkillCommands(skillCommands: CommandDef[]): CommandDef[] {
  quickChatPinsRevision.value;

  const available = skillCommands.filter((command) => command.commandType === "skill" && command.skill);
  if (available.length === 0) return [];

  const stored = loadQuickChatSkillPins();
  const byKey = new Map<string, CommandDef>();
  for (const command of available) {
    const key = commandSkillKey(command);
    if (key) byKey.set(key, command);
  }

  const result: CommandDef[] = [];
  const used = new Set<string>();

  for (const pin of resolveOrderedPins(available)) {
    const command = byKey.get(skillPinKey(pin));
    if (!command || used.has(command.name)) continue;
    result.push(command);
    used.add(command.name);
    if (result.length >= MAX_QUICK_CHAT_SKILLS) return result;
  }

  // User explicitly cleared or customized pins — do not refill the bar.
  if (stored !== null) return result;

  const fallback = [...available].sort((left, right) =>
    (left.skill?.name || left.name).localeCompare(right.skill?.name || right.name, undefined, {
      sensitivity: "base",
    }),
  );
  for (const command of fallback) {
    if (used.has(command.name)) continue;
    result.push(command);
    used.add(command.name);
    if (result.length >= MAX_QUICK_CHAT_SKILLS) break;
  }

  return result;
}

export function useQuickChatSkills(
  skills: Ref<SkillManifest[] | undefined>,
  agentId: Ref<string>,
) {
  const { commands } = useCommandRegistry(skills, agentId);

  const quickSkillCommands = computed(() => {
    const skillCommands = commands.value.filter(
      (command) => command.commandType === "skill" && command.skill,
    );
    return resolveQuickChatSkillCommands(skillCommands);
  });

  return {
    quickSkillCommands,
  };
}
