import { describe, expect, it } from "vitest";
import { formatCodeSelectionForComposer } from "../composables/codePreviewSelection";

describe("codePreviewSelection", () => {
  it("formats selection with file path and fenced code block", () => {
    const payload = formatCodeSelectionForComposer(
      { text: "local x = 1", lineRange: { start: 2, end: 3 } },
      { filePath: "Assets/foo.lua", language: "lua", lineOffset: 10 },
    );
    expect(payload).toContain("Assets/foo.lua:11-12");
    expect(payload).toContain("```lua");
    expect(payload).toContain("local x = 1");
  });

  it("omits location header when file path is unknown", () => {
    const payload = formatCodeSelectionForComposer(
      { text: "print('hi')", lineRange: null },
      { language: "python" },
    );
    expect(payload).toBe("```python\nprint('hi')\n```");
  });
});
