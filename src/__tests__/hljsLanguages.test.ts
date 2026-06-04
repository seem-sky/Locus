import { describe, expect, it } from "vitest";
import hljs, { langFromPath } from "../hljs";

describe("hljs languages", () => {
  it("registers lua for asset and diff previews", () => {
    expect(hljs.getLanguage("lua")).toBeTruthy();
    expect(langFromPath("Assets/Scripts/foo.lua")).toBe("lua");
    expect(langFromPath("Assets.Lua/Games/bar.lua")).toBe("lua");
  });
});
