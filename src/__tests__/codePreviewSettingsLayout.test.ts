import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const cwd = process.cwd();

function read(relPath: string) {
  return readFileSync(resolve(cwd, relPath), "utf8");
}

describe("code preview display settings", () => {
  it("persists typography and applies css variables", () => {
    const displaySettings = read("src/composables/useDisplaySettings.ts");
    const displayPanel = read("src/components/settings/DisplaySettings.vue");
    const typography = read("src/styles/typography.css");
    const zh = read("src/language/zh.json");

    expect(displaySettings).toContain("codePreview: CodePreviewTypography");
    expect(displaySettings).toContain("applyCodePreviewTypography");
    expect(displaySettings).toContain("setCodePreview");
    expect(displaySettings).toContain("resetCodePreview");
    expect(displaySettings).toContain("--code-preview-font-size");

    expect(displayPanel).toContain("settings.display.codePreviewTitle");
    expect(displayPanel).toContain("code-preview-sample");
    expect(displayPanel).toContain("@input=\"onCodePreviewFontSizeInput\"");

    expect(typography).toContain("--code-preview-line-height");

    expect(zh).toContain('"settings.display.codePreviewTitle": "代码预览"');
  });

  it("wires preview surfaces to code preview tokens", () => {
    const assetViewer = read("src/components/asset/AssetTextViewer.vue");
    const diffViewer = read("src/components/diff/FileDiffViewer.vue");

    expect(assetViewer).toContain("code-preview-surface");
    expect(diffViewer).toContain("code-preview-surface");
    expect(diffViewer).toContain("var(--code-preview-font-size)");
  });
});
