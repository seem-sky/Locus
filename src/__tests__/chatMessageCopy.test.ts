import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import type { ChatMessage, ToolCallDisplay } from "../types";
import {
  buildChatMessageClipboardPayloadWithTarget,
  formatToolCallForCopy,
  parseToolCallIdsFromDataset,
  resolveThinkingCopyText,
  resolveToolCallsCopyText,
} from "../composables/chatMessageCopy";

const cwd = process.cwd();

function read(relPath: string) {
  return readFileSync(resolve(cwd, relPath), "utf8");
}

describe("chatMessageCopy", () => {
  it("wires segment-aware copy into ChatView", () => {
    const chatView = read("src/components/ChatView.vue");
    expect(chatView).toContain("parseMessageCopyTargetFromElement");
    expect(chatView).toContain("buildChatMessageClipboardPayloadWithTarget");
    expect(chatView).toContain("copyTarget: MessageCopyTarget");
  });

  it("parses tool-call ids from dataset attributes", () => {
    expect(parseToolCallIdsFromDataset("tc-1, tc-2")).toEqual(["tc-1", "tc-2"]);
    expect(parseToolCallIdsFromDataset(undefined)).toEqual([]);
  });

  it("copies full thinking content instead of the visible summary", () => {
    const message: ChatMessage = {
      id: "assistant-1",
      role: "assistant",
      createdAt: 1,
      content: "最终回复",
      renderParts: [{
        kind: "thinking",
        id: "think-1",
        order: { runId: "run-1", seq: 1 },
        content: "第一步：分析需求\n第二步：查找文件",
        duration: 3,
      }],
    };

    const text = resolveThinkingCopyText(
      { kind: "thinking", renderPartKey: "assistant-1:think-1", scope: "history" },
      { messages: [message] },
      message,
    );

    expect(text).toBe("第一步：分析需求\n第二步：查找文件");
  });

  it("copies the full tool-call process including arguments and output", () => {
    const messages: ChatMessage[] = [
      {
        id: "assistant-1",
        role: "assistant",
        createdAt: 1,
        content: "",
        toolCalls: [{
          id: "tc-1",
          name: "read",
          arguments: "{\"filePath\":\"src/main.ts\"}",
          order: 1,
        }],
      },
      {
        id: "tool-1",
        role: "tool",
        createdAt: 2,
        content: "console.log('hello');",
        toolCallId: "tc-1",
      },
    ];

    const text = resolveToolCallsCopyText(
      { kind: "toolCall", toolCallIds: ["tc-1"], scope: "history" },
      { messages },
    );

    expect(text).toContain("Tool: read");
    expect(text).toContain("Status: done");
    expect(text).toContain("\"filePath\":\"src/main.ts\"");
    expect(text).toContain("console.log('hello');");
  });

  it("builds clipboard payloads for segment-aware copy targets", () => {
    const activeToolCalls: ToolCallDisplay[] = [{
      id: "tc-live",
      name: "grep",
      arguments: "{\"pattern\":\"foo\"}",
      status: "running",
    }];

    const payload = buildChatMessageClipboardPayloadWithTarget(
      null,
      { kind: "toolCall", toolCallIds: ["tc-live"], scope: "transient" },
      { messages: [], activeToolCalls },
    );

    expect(payload.text).toContain("Tool: grep");
    expect(payload.text).toContain("Status: running");
    expect(payload.text).toContain("Output: (running)");
    expect(payload.serializedDraft).toBeNull();
  });

  it("formats nested tool calls in copy output", () => {
    const text = formatToolCallForCopy({
      id: "tc-parent",
      name: "Task",
      arguments: "{}",
      status: "done",
      output: "done",
      nestedToolCalls: [{
        id: "tc-child",
        name: "read",
        arguments: "{\"filePath\":\"a.ts\"}",
        status: "done",
        output: "content",
      }],
    });

    expect(text).toContain("Tool: Task");
    expect(text).toContain("  Tool: read");
    expect(text).toContain("  Output:");
    expect(text).toContain("  content");
  });
});
