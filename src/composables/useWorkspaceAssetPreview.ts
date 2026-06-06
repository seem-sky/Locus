import { computed, ref, watch, type Ref } from "vue";
import {
  previewWorkspaceAsset,
  previewWorkspaceAssetTarget,
} from "../services/asset";
import { normalizeAppError } from "../services/errors";
import type { AssetPreviewPayload, SemanticTargetInspector } from "../types";
import { defaultStructuredTargetId } from "./assetPreviewTarget";

export function useWorkspaceAssetPreview(
  workingDir: Ref<string>,
  assetPath: Ref<string | null>,
) {
  const previewPayload = ref<AssetPreviewPayload | null>(null);
  const previewLoading = ref(false);
  const previewError = ref("");
  const previewName = ref("");
  const activeTargetId = ref<string | null>(null);
  const targetCache = ref<Map<string, SemanticTargetInspector>>(new Map());
  const targetLoading = ref(false);

  let previewSession = 0;
  let targetRequestGeneration = 0;

  const hasWorkspace = computed(() => !!workingDir.value.trim());
  const previewDisplayPath = computed(() => assetPath.value ?? "");
  const previewDisplayName = computed(() => {
    if (previewName.value.trim()) return previewName.value.trim();
    const path = previewDisplayPath.value;
    if (!path) return "";
    const segments = path.split("/").filter(Boolean);
    return segments[segments.length - 1] ?? path;
  });

  function invalidatePreviewSession(): number {
    previewSession += 1;
    targetRequestGeneration += 1;
    return previewSession;
  }

  function clearPreview() {
    invalidatePreviewSession();
    previewPayload.value = null;
    previewLoading.value = false;
    previewError.value = "";
    previewName.value = "";
    activeTargetId.value = null;
    targetCache.value = new Map();
    targetLoading.value = false;
  }

  async function loadPreview(path: string, displayName?: string) {
    const normalizedPath = path.trim().replace(/\\/g, "/");
    if (!hasWorkspace.value || !normalizedPath) {
      clearPreview();
      return;
    }

    previewName.value = displayName?.trim() || "";
    const session = invalidatePreviewSession();
    previewLoading.value = true;
    previewError.value = "";
    targetLoading.value = false;
    previewPayload.value = null;
    activeTargetId.value = null;
    targetCache.value = new Map();

    try {
      const payload = await previewWorkspaceAsset(normalizedPath, { textScope: "full" });
      if (session !== previewSession) return;
      previewPayload.value = payload;
      const defaultTargetId = defaultStructuredTargetId(payload);
      if (payload.kind === "structured" && defaultTargetId) {
        await loadTarget(payload.previewKey, defaultTargetId, session);
      }
    } catch (error) {
      if (session !== previewSession) return;
      const err = normalizeAppError(error);
      previewPayload.value = null;
      previewError.value = err.message;
    } finally {
      if (session === previewSession) {
        previewLoading.value = false;
        targetLoading.value = false;
      }
    }
  }

  async function loadTarget(previewKey: string, targetId: string, session = previewSession) {
    if (!hasWorkspace.value) return null;
    const generation = ++targetRequestGeneration;
    activeTargetId.value = targetId;
    const cached = targetCache.value.get(targetId);
    if (cached) {
      targetLoading.value = false;
      return cached;
    }

    targetLoading.value = true;
    try {
      const inspector = await previewWorkspaceAssetTarget(previewKey, targetId);
      if (session !== previewSession) return null;
      const payload = previewPayload.value;
      if (!payload || payload.kind !== "structured" || payload.previewKey !== previewKey) {
        return null;
      }
      const next = new Map(targetCache.value);
      next.set(targetId, inspector);
      targetCache.value = next;
      if (generation === targetRequestGeneration) {
        activeTargetId.value = targetId;
      }
      return inspector;
    } catch (error) {
      if (session !== previewSession || generation !== targetRequestGeneration) return null;
      const err = normalizeAppError(error);
      if (
        err.code === "asset.preview.cache_miss"
        && err.retryable
        && assetPath.value
      ) {
        await loadPreview(assetPath.value, previewName.value);
        const newPayload = previewPayload.value;
        if (newPayload && newPayload.kind === "structured") {
          return loadTarget(newPayload.previewKey, targetId, session);
        }
      }
      previewError.value = err.message;
      return null;
    } finally {
      if (session === previewSession && generation === targetRequestGeneration) {
        targetLoading.value = false;
      }
    }
  }

  watch(
    [workingDir, assetPath],
    ([dir, path]) => {
      if (!dir.trim() || !path) {
        clearPreview();
        return;
      }
      void loadPreview(path);
    },
    { immediate: true },
  );

  return {
    previewPayload,
    previewLoading,
    previewError,
    previewDisplayName,
    previewDisplayPath,
    activeTargetId,
    targetCache,
    targetLoading,
    loadTarget,
    clearPreview,
  };
}
