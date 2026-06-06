import { ref } from "vue";
import { t } from "../i18n";
import { useCopyFeedback } from "./useCopyFeedback";
import { useChatStore } from "../stores/chat";
import { useUiStore } from "../stores/ui";
import { useNotificationStore } from "../stores/notification";
import {
  findCodePreviewSurface,
  formatCodeSelectionForComposer,
  getCodePreviewSelectionContext,
  type CodePreviewSelectionMeta,
} from "./codePreviewSelection";

export interface CodePreviewSelectionMenuState {
  x: number;
  y: number;
  text: string;
  meta: CodePreviewSelectionMeta;
  lineRange: { start: number; end: number } | null;
}

export function useCodePreviewSelectionMenu(defaultMeta: () => CodePreviewSelectionMeta) {
  const chatStore = useChatStore();
  const uiStore = useUiStore();
  const notificationStore = useNotificationStore();
  const { copyText } = useCopyFeedback();
  const menu = ref<CodePreviewSelectionMenuState | null>(null);

  function closeMenu() {
    menu.value = null;
  }

  function handleContextMenu(event: MouseEvent) {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const surface = findCodePreviewSurface(target);
    if (!surface) return;

    const context = getCodePreviewSelectionContext(surface);
    if (!context) return;

    event.preventDefault();
    event.stopPropagation();
    menu.value = {
      x: event.clientX,
      y: event.clientY,
      text: context.text,
      meta: defaultMeta(),
      lineRange: context.lineRange,
    };
  }

  async function copySelection() {
    const current = menu.value;
    if (!current) return;
    closeMenu();
    const ok = await copyText(current.text);
    if (!ok) {
      notificationStore.addNotice("error", t("codePreview.selection.copyFailed"), {
        operation: "codePreviewCopy",
      });
    }
  }

  function sendToComposer() {
    const current = menu.value;
    if (!current) return;
    closeMenu();
    const payload = formatCodeSelectionForComposer(
      { text: current.text, lineRange: current.lineRange },
      current.meta,
    );
    chatStore.closeFloatingAssetPreview();
    uiStore.setTab("chat");
    uiStore.stageChatPrefillAppend(payload);
  }

  return {
    menu,
    closeMenu,
    handleContextMenu,
    copySelection,
    sendToComposer,
  };
}
