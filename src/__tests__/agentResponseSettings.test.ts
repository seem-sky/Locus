import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { resolveChatResponseLocale, useAgentResponseSettings } from "../composables/useAgentResponseSettings";

const cwd = process.cwd();

function read(relPath: string) {
  return readFileSync(resolve(cwd, relPath), "utf8");
}

describe("agent response settings", () => {
  it("forces zh response locale when enabled", () => {
    const { set } = useAgentResponseSettings();
    set("forceChineseChat", true);
    expect(resolveChatResponseLocale("en")).toBe("zh");
    set("forceChineseChat", false);
    expect(resolveChatResponseLocale("en")).toBe("en");
  });

  it("adds a force Chinese chat toggle in general settings", () => {
    const settings = read("src/composables/useAgentResponseSettings.ts");
    const general = read("src/components/settings/GeneralSettings.vue");
    const chatStore = read("src/stores/chat.ts");
    const zh = read("src/language/zh.json");
    const en = read("src/language/en.json");

    expect(settings).toContain("forceChineseChat: boolean;");
    expect(settings).toContain("forceChineseChat: false,");
    expect(settings).toContain("export function resolveChatResponseLocale");

    expect(general).toContain("useAgentResponseSettings");
    expect(general).toContain("settings.general.sessionTitle");
    expect(general).toContain("forceChineseChat");
    expect(general).toContain("setAgentResponseSetting('forceChineseChat', $event)");

    expect(chatStore).toContain("resolveChatResponseLocale");
    expect(chatStore).toContain("responseLocale: resolveChatResponseLocale(locale.value)");

    expect(zh).toContain('"settings.general.sessionTitle": "会话设置"');
    expect(zh).toContain('"settings.general.forceChineseChat": "强制使用中文对话思考"');
    expect(en).toContain('"settings.general.forceChineseChat": "Force Chinese for chat and thinking"');
  });
});
