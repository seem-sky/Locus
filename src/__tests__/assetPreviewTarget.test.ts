import { describe, expect, it } from "vitest";
import { defaultStructuredTargetId } from "../composables/assetPreviewTarget";
import type { AssetPreviewPayload } from "../types";

describe("defaultStructuredTargetId", () => {
  it("prefers prefab root when present", () => {
    const payload = {
      kind: "structured",
      previewKey: "k",
      layout: "sceneHierarchyInspector",
      tree: [
        { id: "root", parentId: null, label: "Root", hasInspector: true, objectKind: "gameObject", path: "Root", childIds: ["child"] },
        { id: "child", parentId: "root", label: "Child", hasInspector: true, objectKind: "gameObject", path: "Root/Child", childIds: [] },
      ],
      targets: [],
    } as AssetPreviewPayload;

    expect(defaultStructuredTargetId(payload)).toBe("root");
  });

  it("selects first inspectable yaml doc when no prefab root", () => {
    const payload = {
      kind: "structured",
      previewKey: "k",
      layout: "yamlDocs",
      tree: [
        { id: "doc:1", parentId: null, label: "Material", hasInspector: true, objectKind: "assetRoot", path: "Material", childIds: [] },
      ],
      targets: [],
    } as AssetPreviewPayload;

    expect(defaultStructuredTargetId(payload)).toBe("doc:1");
  });
});
