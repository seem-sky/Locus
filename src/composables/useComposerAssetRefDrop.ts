import { inject, onUnmounted, provide, type InjectionKey } from "vue";
import type { AssetRefAttachment } from "../types";
import {
  acceptAssetRefDragEvent,
  endAssetRefDrag,
  resolveAssetRefDrop,
} from "./assetRefDrag";
import { buildWorkspaceAssetRef } from "./workspaceAssetRef";

export type ComposerAssetRefDropHandler = (refs: AssetRefAttachment[]) => void;

export const composerAssetRefDropKey: InjectionKey<ComposerAssetRefDropHandler> = Symbol(
  "composerAssetRefDrop",
);

let registeredComposerDrop: ComposerAssetRefDropHandler | null = null;

export function registerComposerAssetRefDropHandler(handler: ComposerAssetRefDropHandler | null) {
  registeredComposerDrop = handler;
}

export function commitComposerAssetRefDrop(path: string): boolean {
  const assetRef = buildWorkspaceAssetRef(path);
  if (!assetRef) return false;
  const handler = registeredComposerDrop;
  if (!handler) return false;
  handler([assetRef]);
  endAssetRefDrag();
  return true;
}

export function provideComposerAssetRefDrop(handler: ComposerAssetRefDropHandler) {
  provide(composerAssetRefDropKey, handler);
  registerComposerAssetRefDropHandler(handler);
  onUnmounted(() => {
    if (registeredComposerDrop === handler) {
      registerComposerAssetRefDropHandler(null);
    }
  });
}

export function useComposerAssetRefDropTarget() {
  const dropToComposer = inject(composerAssetRefDropKey, null);

  function acceptDragOver(event: DragEvent): boolean {
    return acceptAssetRefDragEvent(event);
  }

  function handleDrop(event: DragEvent): boolean {
    const payload = resolveAssetRefDrop(event);
    if (!payload || !dropToComposer) return false;
    const assetRef = buildWorkspaceAssetRef(payload.path);
    if (!assetRef) return false;
    dropToComposer([assetRef]);
    return true;
  }

  return {
    acceptDragOver,
    handleDrop,
  };
}
