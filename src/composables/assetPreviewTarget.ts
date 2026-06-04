import type { AssetPreviewPayload } from "../types";

/** Pick the first structured target to show inspector content on open. */
export function defaultStructuredTargetId(payload: AssetPreviewPayload): string | null {
  if (payload.kind !== "structured" || payload.tree.length === 0) return null;

  const knownIds = new Set(payload.tree.map((node) => node.id));
  const prefabRoot = payload.tree.find(
    (node) => node.hasInspector && (!node.parentId || !knownIds.has(node.parentId)),
  );
  if (prefabRoot) return prefabRoot.id;

  const firstInspectable = payload.tree.find((node) => node.hasInspector);
  if (firstInspectable) return firstInspectable.id;

  return payload.tree[0]?.id ?? null;
}
