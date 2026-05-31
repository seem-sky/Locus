import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const cwd = process.cwd();

function read(relPath: string) {
  return readFileSync(resolve(cwd, relPath), "utf8");
}

describe("chat project view panel", () => {
  it("wires embedded asset tree, floating preview, and drag-to-chat refs", () => {
    const chatView = read("src/components/ChatView.vue");
    const workspace = read("src/components/ChatWorkspaceView.vue");
    const panel = read("src/components/ChatProjectViewPanel.vue");
    const assetView = read("src/components/AssetView.vue");
    const legacyExplorer = read("src/components/asset/AssetLegacyExplorer.vue");
    const floatingPreview = read("src/components/chat/ChatFloatingAssetPreview.vue");
    const richInput = read("src/components/chat/RichChatInput.vue");
    const chatStore = read("src/stores/chat.ts");
    const assetRefDrag = read("src/composables/assetRefDrag.ts");
    const composerDrop = read("src/composables/useComposerAssetRefDrop.ts");
    const pointerDrag = read("src/composables/useAssetRefPointerDrag.ts");
    const zh = read("src/language/zh.json");

    expect(chatStore).toContain("const floatingAssetPreview = ref");
    expect(chatStore).toContain("function openFloatingAssetPreview");
    expect(chatStore).toContain("function closeFloatingAssetPreview()");

    expect(chatView).toContain("ChatFloatingAssetPreview");
    expect(chatView).toContain("floatingAssetPreview && workingDir");

    expect(workspace).toContain("ChatProjectViewPanel");
    expect(panel).toContain("<AssetView :working-dir=\"workingDir\" embedded />");

    expect(assetView).toContain("handleEmbeddedPreview");
    expect(assetView).toContain("chatStore.openFloatingAssetPreview");
    expect(assetView).toContain("asset-ref-draggable");
    expect(assetView).not.toContain("ax-pane-embedded-preview");

    expect(legacyExplorer).toContain('emit("preview", entry.node)');
    expect(legacyExplorer).toContain("useAssetRefPointerDragSource");
    expect(legacyExplorer).toContain("@pointerdown=\"onAssetRowPointerDown(entry, $event)\"");
    expect(legacyExplorer).toContain(':data-asset-ref-path="assetRefDraggable ? entry.node.path : undefined"');
    expect(legacyExplorer).toContain(':draggable="false"');
    expect(legacyExplorer).toContain("@dblclick.stop=\"rowDblClick(entry)\"");
    expect(legacyExplorer).toContain('role="button"');

    expect(floatingPreview).toContain("chat-floating-asset-preview");
    expect(floatingPreview).toContain("inset: 0");
    expect(chatView).toMatch(/<\/RichChatInput>\s*<\/div>\s*<ChatFloatingAssetPreview/);
    expect(floatingPreview).toContain("useAssetRefPointerDragSource");
    expect(floatingPreview).toContain("data-composer-asset-ref-drop");
    expect(floatingPreview).toContain("@pointerdown.stop=\"onHeaderPointerDown\"");
    expect(floatingPreview).toContain("@dragover=\"onPreviewDragOver\"");
    expect(floatingPreview).toContain("@drop=\"onPreviewDrop\"");
    expect(floatingPreview).toContain("useWorkspaceAssetPreview");

    expect(assetRefDrag).toContain("LOCUS_ASSET_REF_DRAG_MIME");
    expect(assetRefDrag).toContain("draggingAssetRefPath");
    expect(pointerDrag).toContain("COMPOSER_ASSET_REF_DROP_SELECTOR");
    expect(pointerDrag).toContain("commitComposerAssetRefDrop");
    expect(composerDrop).toContain("provideComposerAssetRefDrop");
    expect(composerDrop).toContain("commitComposerAssetRefDrop");
    expect(composerDrop).toContain("useComposerAssetRefDropTarget");
    expect(chatView).toContain("provideComposerAssetRefDrop");
    expect(chatView).toContain("data-composer-asset-ref-drop");
    expect(richInput).toContain("useComposerAssetRefDropTarget");
    expect(richInput).toContain("resolveAssetRefDrop");
    expect(richInput).toContain("handleComposerDrop");
    expect(richInput).toContain("addAssetRefs,");

    expect(zh).toContain('"chat.floatingAssetPreview.dragHint": "拖到输入框引用"');
  });
});
