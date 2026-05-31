import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_QUICK_CHAT_SKILL_DIR_NAMES,
  isQuickChatSkillPinned,
  pinQuickChatSkill,
  QUICK_CHAT_SKILLS_STORAGE_KEY,
  resolveQuickChatSkillCommands,
  unpinQuickChatSkill,
} from "../composables/useQuickChatSkills";
import type { SkillManifest } from "../types";
import type { CommandDef } from "../composables/chatInputIntents";

const cwd = process.cwd();

function read(relPath: string) {
  return readFileSync(resolve(cwd, relPath), "utf8");
}

function skillCommand(
  dirName: string,
  name: string,
  source: "app" | "project" = "project",
): CommandDef {
  return {
    name: `/${dirName}`,
    description: `${name} description`,
    commandKind: "intent",
    commandType: "skill",
    skill: {
      dirName,
      source,
      name,
    },
  };
}

function createStorageMock() {
  const store = new Map<string, string>();
  return {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
    clear: () => {
      store.clear();
    },
  };
}

describe("quick chat skills", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", createStorageMock());
  });

  it("prefers the default workspace skill order when available", () => {
    const commands = [
      skillCommand("code-reviewer", "Code Reviewer"),
      skillCommand("writing-plans", "Writing Plans"),
      skillCommand("systematic-debugging", "Systematic Debugging"),
    ];

    const resolved = resolveQuickChatSkillCommands(commands);
    expect(resolved.map((item) => item.skill?.dirName)).toEqual([
      "systematic-debugging",
      "writing-plans",
      "code-reviewer",
    ]);
  });

  it("fills remaining slots with other command-enabled skills", () => {
    const commands = [
      skillCommand("alpha-skill", "Alpha"),
      skillCommand("beta-skill", "Beta"),
    ];

    const resolved = resolveQuickChatSkillCommands(commands);
    expect(resolved).toHaveLength(2);
    expect(resolved[0].skill?.dirName).toBe("alpha-skill");
  });

  it("wires the quick skill bar into the chat composer footer", () => {
    const richInput = read("src/components/chat/RichChatInput.vue");
    const composerQuick = read("src/components/chat/ComposerQuickSkills.vue");
    expect(richInput).toContain("ComposerQuickSkills");
    expect(richInput).toContain("useQuickChatSkills");
    expect(richInput).toContain("@toggle=\"toggleQuickSkill\"");
    expect(richInput).toContain("@unpin=\"handleQuickSkillUnpin\"");
    expect(composerQuick).toContain("@contextmenu.prevent=\"openContextMenu($event, command)\"");
    expect(composerQuick).toContain("chat.quickSkills.removeFromBar");
    expect(composerQuick).toContain("unpinQuickChatSkill(pin, props.skills, props.commands)");
    expect(DEFAULT_QUICK_CHAT_SKILL_DIR_NAMES).toContain("systematic-debugging");
  });

  it("does not refill the quick bar after an explicit empty pin list", () => {
    const commands = [skillCommand("alpha-skill", "Alpha")];
    localStorage.setItem(QUICK_CHAT_SKILLS_STORAGE_KEY, "[]");
    expect(resolveQuickChatSkillCommands(commands)).toEqual([]);
  });

  it("unpin removes a skill from the visible quick bar baseline", () => {
    const skills: SkillManifest[] = [
      {
        name: "Alpha",
        description: "",
        argumentHint: "",
        dirName: "alpha-skill",
        source: "project",
        relPath: "alpha-skill.md",
        updatedAt: 0,
        skillEnabled: true,
        skillSurface: "command",
        skillDescription: null,
        commandTrigger: "/alpha-skill",
      },
      {
        name: "Beta",
        description: "",
        argumentHint: "",
        dirName: "beta-skill",
        source: "project",
        relPath: "beta-skill.md",
        updatedAt: 0,
        skillEnabled: true,
        skillSurface: "command",
        skillDescription: null,
        commandTrigger: "/beta-skill",
      },
    ];
    const commands = [
      skillCommand("alpha-skill", "Alpha"),
      skillCommand("beta-skill", "Beta"),
    ];

    unpinQuickChatSkill(
      { source: "project", dirName: "alpha-skill" },
      skills,
      commands,
    );
    expect(
      resolveQuickChatSkillCommands(commands).map((item) => item.skill?.dirName),
    ).toEqual(["beta-skill"]);
  });

  it("pins and unpins skills for the chat quick bar", () => {
    const skills: SkillManifest[] = [
      {
        name: "Alpha",
        description: "",
        argumentHint: "",
        dirName: "alpha-skill",
        source: "project",
        relPath: "alpha-skill.md",
        updatedAt: 0,
        skillEnabled: true,
        skillSurface: "command",
        skillDescription: null,
        commandTrigger: "/alpha-skill",
      },
      {
        name: "Beta",
        description: "",
        argumentHint: "",
        dirName: "beta-skill",
        source: "project",
        relPath: "beta-skill.md",
        updatedAt: 0,
        skillEnabled: true,
        skillSurface: "command",
        skillDescription: null,
        commandTrigger: "/beta-skill",
      },
    ];

    const alphaPin = { source: "project" as const, dirName: "alpha-skill" };
    expect(isQuickChatSkillPinned(alphaPin, skills)).toBe(false);

    pinQuickChatSkill(alphaPin, skills);
    expect(isQuickChatSkillPinned(alphaPin, skills)).toBe(true);
    expect(
      resolveQuickChatSkillCommands([skillCommand("alpha-skill", "Alpha")]).map(
        (item) => item.skill?.dirName,
      ),
    ).toEqual(["alpha-skill"]);

    unpinQuickChatSkill(alphaPin, skills);
    expect(isQuickChatSkillPinned(alphaPin, skills)).toBe(false);
  });

  it("exposes quick chat pin controls in skill preview settings", () => {
    const preview = read("src/components/knowledge/KnowledgePreview.vue");
    const packagePreview = read("src/components/knowledge/KnowledgeSkillPackagePreview.vue");
    expect(preview).toContain("knowledge.skill.quickChatPin");
    expect(preview).toContain("onSkillQuickChatPinChange");
    expect(preview).toContain("showSkillQuickChatPin");
    expect(preview).toContain("skillQuickChatPinRequiresCommand");
    expect(preview).not.toContain("isReadOnly.value || props.saveLoading || !showSkillCommandFields");
    expect(packagePreview).toContain("onSkillQuickChatPinChange");
  });
});
