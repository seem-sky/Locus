import type { AssetRefAttachment } from "../types";

const PROJECT_KNOWLEDGE_REF_ROOT_RE = /^(?:design|memory|skill|reference)\/.+\.md$/i;
const WINDOWS_DRIVE_ABSOLUTE_RE = /^[A-Za-z]:[\\/]/;
const UNC_ABSOLUTE_RE = /^(?:\\\\|\/\/)/;
const POSIX_ABSOLUTE_RE = /^\/(?!\/)/;

function isWorkspaceFolderRefPath(path: string): boolean {
  const segments = path.split("/").filter(Boolean);
  if (segments.length === 0) return false;
  const last = segments[segments.length - 1] ?? "";
  return !last.includes(".");
}

export function normalizeWorkspaceAssetRefPath(path: string): string {
  return path.trim().replace(/\\/g, "/").replace(/\/+$/, "");
}

export function isValidWorkspaceRelativeRefPath(path: string): boolean {
  const normalized = normalizeWorkspaceAssetRefPath(path);
  if (!normalized) return false;
  if (
    WINDOWS_DRIVE_ABSOLUTE_RE.test(normalized)
    || UNC_ABSOLUTE_RE.test(normalized)
    || POSIX_ABSOLUTE_RE.test(normalized)
  ) {
    return false;
  }

  const segments = normalized.split("/").filter(Boolean);
  if (segments.length === 0) return false;

  return segments.every((segment) => segment !== "." && segment !== "..");
}

export function inferWorkspaceAssetRefKind(
  path: string,
  kind?: AssetRefAttachment["kind"],
): AssetRefAttachment["kind"] {
  if (kind === "knowledge") return "knowledge";
  if (kind === "sceneObject") return "sceneObject";
  if (/^((?:Assets|Packages)\/.+?\.unity)\/.+/i.test(path)) {
    return "sceneObject";
  }
  return "asset";
}

export function isSupportedWorkspaceAssetRefPath(path: string): boolean {
  return isValidWorkspaceRelativeRefPath(normalizeWorkspaceAssetRefPath(path));
}

export function buildWorkspaceAssetRef(path: string): AssetRefAttachment | null {
  const normalizedPath = normalizeWorkspaceAssetRefPath(path);
  if (!isSupportedWorkspaceAssetRefPath(normalizedPath)) return null;

  if (PROJECT_KNOWLEDGE_REF_ROOT_RE.test(normalizedPath)) {
    return {
      path: normalizedPath,
      kind: "knowledge",
      source: "manual",
    };
  }

  const assetRef: AssetRefAttachment = {
    path: normalizedPath,
    kind: inferWorkspaceAssetRefKind(normalizedPath),
    source: "manual",
  };

  if (isWorkspaceFolderRefPath(normalizedPath)) {
    assetRef.typeLabel = "Folder";
  }

  return assetRef;
}
