import { normalizeWorkspaceAssetRefPath } from "./workspaceAssetRef";

export const LOCUS_ASSET_REF_DRAG_MIME = "application/x-locus-asset-ref";
const LOCUS_ASSET_REF_PLAIN_PREFIX = "locus-asset-ref:";

export interface LocusAssetRefDragPayload {
  path: string;
}

let draggingAssetRefPath: string | null = null;

export function beginAssetRefDrag(path: string) {
  const normalized = normalizeWorkspaceAssetRefPath(path);
  draggingAssetRefPath = normalized || null;
}

export function endAssetRefDrag() {
  draggingAssetRefPath = null;
}

export function getDraggingAssetRefPath(): string | null {
  return draggingAssetRefPath;
}

export function isAssetRefDragActive(dataTransfer: DataTransfer | null): boolean {
  if (draggingAssetRefPath) return true;
  if (!dataTransfer) return false;
  const types = Array.from(dataTransfer.types);
  if (types.includes(LOCUS_ASSET_REF_DRAG_MIME)) return true;
  return types.includes("text/plain");
}

export function setAssetRefDragData(dataTransfer: DataTransfer, path: string) {
  const normalized = normalizeWorkspaceAssetRefPath(path);
  if (!normalized) return;
  const payload: LocusAssetRefDragPayload = { path: normalized };
  dataTransfer.setData(LOCUS_ASSET_REF_DRAG_MIME, JSON.stringify(payload));
  dataTransfer.setData("text/plain", `${LOCUS_ASSET_REF_PLAIN_PREFIX}${normalized}`);
  dataTransfer.effectAllowed = "copy";
}

export function readAssetRefDragPayload(dataTransfer: DataTransfer | null): LocusAssetRefDragPayload | null {
  if (!dataTransfer) return null;

  const raw = dataTransfer.getData(LOCUS_ASSET_REF_DRAG_MIME);
  if (raw) {
    try {
      const parsed = JSON.parse(raw) as LocusAssetRefDragPayload;
      const path = typeof parsed.path === "string"
        ? normalizeWorkspaceAssetRefPath(parsed.path)
        : "";
      if (path) return { path };
    } catch {
      // fall through to plain-text payload
    }
  }

  const plain = dataTransfer.getData("text/plain").trim();
  if (plain.startsWith(LOCUS_ASSET_REF_PLAIN_PREFIX)) {
    const path = normalizeWorkspaceAssetRefPath(plain.slice(LOCUS_ASSET_REF_PLAIN_PREFIX.length));
    if (path) return { path };
  }

  if (draggingAssetRefPath) {
    return { path: draggingAssetRefPath };
  }

  return null;
}

export function acceptAssetRefDragEvent(event: DragEvent): boolean {
  if (!isAssetRefDragActive(event.dataTransfer)) return false;
  event.preventDefault();
  event.stopPropagation();
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = "copy";
  }
  return true;
}

export function resolveAssetRefDrop(event: DragEvent): LocusAssetRefDragPayload | null {
  if (!acceptAssetRefDragEvent(event)) return null;
  const payload = readAssetRefDragPayload(event.dataTransfer);
  endAssetRefDrag();
  return payload;
}
