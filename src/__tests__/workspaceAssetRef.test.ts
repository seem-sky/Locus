import { describe, expect, it } from "vitest";
import {
  buildWorkspaceAssetRef,
  isSupportedWorkspaceAssetRefPath,
} from "../composables/workspaceAssetRef";

describe("workspaceAssetRef", () => {
  it("accepts Unity asset paths", () => {
    expect(isSupportedWorkspaceAssetRefPath("Assets/Scripts/Foo.cs")).toBe(true);
    expect(buildWorkspaceAssetRef("Assets/Scripts/Foo.cs")).toMatchObject({
      path: "Assets/Scripts/Foo.cs",
      kind: "asset",
      source: "manual",
    });
  });

  it("accepts other workspace root folders and files", () => {
    expect(isSupportedWorkspaceAssetRefPath("Locus/src/App.vue")).toBe(true);
    expect(buildWorkspaceAssetRef("Locus/src/App.vue")).toMatchObject({
      path: "Locus/src/App.vue",
      kind: "asset",
      source: "manual",
    });

    expect(buildWorkspaceAssetRef("design")).toMatchObject({
      path: "design",
      kind: "asset",
      typeLabel: "Folder",
      source: "manual",
    });
  });

  it("rejects absolute paths", () => {
    expect(isSupportedWorkspaceAssetRefPath("C:/outside/file.txt")).toBe(false);
    expect(buildWorkspaceAssetRef("C:/outside/file.txt")).toBeNull();
  });
});
