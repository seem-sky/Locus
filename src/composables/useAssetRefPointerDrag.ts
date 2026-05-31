import { beginAssetRefDrag, endAssetRefDrag } from "./assetRefDrag";
import { commitComposerAssetRefDrop } from "./useComposerAssetRefDrop";

/** Drop zones for workspace asset refs dragged from the project tree. */
export const COMPOSER_ASSET_REF_DROP_SELECTOR =
  "[data-composer-asset-ref-drop], .chat-input-shell, .input-area, .chat-floating-asset-preview";

const DRAG_THRESHOLD_PX = 5;
const BODY_DRAGGING_CLASS = "asset-ref-pointer-dragging";
const DROP_HOVER_CLASS = "is-asset-ref-drop-hover";

interface AssetRefPointerDragState {
  path: string;
  pointerId: number;
  startX: number;
  startY: number;
  active: boolean;
}

let pointerDragState: AssetRefPointerDragState | null = null;
let pointerMoveListener: ((event: PointerEvent) => void) | null = null;
let pointerUpListener: ((event: PointerEvent) => void) | null = null;
let pointerCancelListener: (() => void) | null = null;
let suppressNextClick = false;
let suppressNextClickTimer = 0;
let hoveredDropElement: HTMLElement | null = null;

function scheduleSuppressNextClick() {
  suppressNextClick = true;
  if (suppressNextClickTimer) {
    window.clearTimeout(suppressNextClickTimer);
  }
  suppressNextClickTimer = window.setTimeout(() => {
    suppressNextClick = false;
    suppressNextClickTimer = 0;
  }, 240);
}

export function consumeAssetRefPointerClickSuppression(): boolean {
  if (!suppressNextClick) return false;
  suppressNextClick = false;
  if (suppressNextClickTimer) {
    window.clearTimeout(suppressNextClickTimer);
    suppressNextClickTimer = 0;
  }
  return true;
}

function clearDropHover() {
  hoveredDropElement?.classList.remove(DROP_HOVER_CLASS);
  hoveredDropElement = null;
}

function resolveDropTargetFromPoint(x: number, y: number): HTMLElement | null {
  const target = document.elementFromPoint(x, y);
  if (!(target instanceof Element)) return null;
  return target.closest<HTMLElement>(COMPOSER_ASSET_REF_DROP_SELECTOR);
}

function updateDropHover(x: number, y: number) {
  const dropEl = resolveDropTargetFromPoint(x, y);
  if (dropEl === hoveredDropElement) return;
  clearDropHover();
  if (dropEl) {
    dropEl.classList.add(DROP_HOVER_CLASS);
    hoveredDropElement = dropEl;
  }
}

function clearPointerDragListeners() {
  if (pointerMoveListener) {
    window.removeEventListener("pointermove", pointerMoveListener);
    pointerMoveListener = null;
  }
  if (pointerUpListener) {
    window.removeEventListener("pointerup", pointerUpListener);
    pointerUpListener = null;
  }
  if (pointerCancelListener) {
    window.removeEventListener("pointercancel", pointerCancelListener);
    pointerCancelListener = null;
  }
  document.body.classList.remove(BODY_DRAGGING_CLASS);
  clearDropHover();
}

function clearPointerDragState() {
  pointerDragState = null;
  clearPointerDragListeners();
}

function onPointerMove(event: PointerEvent) {
  const state = pointerDragState;
  if (!state || event.pointerId !== state.pointerId) return;

  const dx = event.clientX - state.startX;
  const dy = event.clientY - state.startY;
  if (!state.active) {
    if (Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
    state.active = true;
    beginAssetRefDrag(state.path);
    document.body.classList.add(BODY_DRAGGING_CLASS);
  }

  event.preventDefault();
  updateDropHover(event.clientX, event.clientY);
}

function finishPointerDrag(event: PointerEvent) {
  const state = pointerDragState;
  if (!state || event.pointerId !== state.pointerId) return;

  const wasActive = state.active;
  const dropTarget = wasActive
    ? resolveDropTargetFromPoint(event.clientX, event.clientY)
    : null;

  clearPointerDragState();
  endAssetRefDrag();

  if (wasActive) {
    scheduleSuppressNextClick();
  }

  if (dropTarget) {
    commitComposerAssetRefDrop(state.path);
  }
}

function shouldIgnoreFileRowPointerDrag(event: PointerEvent): boolean {
  const target = event.target;
  return (
    target instanceof Element &&
    !!target.closest(".alx-branch, input, textarea, select, button")
  );
}

export function useAssetRefPointerDragSource() {
  function onFileRowPointerDown(path: string, event: PointerEvent) {
    if (event.button !== 0 || shouldIgnoreFileRowPointerDrag(event)) return;

    clearPointerDragState();
    pointerDragState = {
      path,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      active: false,
    };

    pointerMoveListener = onPointerMove;
    pointerUpListener = finishPointerDrag;
    pointerCancelListener = () => {
      endAssetRefDrag();
      clearPointerDragState();
    };

    window.addEventListener("pointermove", pointerMoveListener);
    window.addEventListener("pointerup", pointerUpListener);
    window.addEventListener("pointercancel", pointerCancelListener);
  }

  return {
    onFileRowPointerDown,
  };
}
