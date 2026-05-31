import type { AssistantRenderPart, ChatMessage, ImageAttachment, ToolCallDisplay, ToolCallInfo } from "../types";
import {
  buildChatMessageClipboardPayload,
  type ChatMessageClipboardPayload,
} from "./chatMessageDraft";
import { buildMessageToolCall } from "./toolCallBatches";

export type MessageCopyTarget =
  | { kind: "message" }
  | { kind: "thinking"; renderPartKey: string; scope: "history" | "transient" }
  | { kind: "toolCall"; toolCallIds: string[]; scope: "history" | "transient" };

export interface ChatCopyContext {
  messages: ChatMessage[];
  liveRenderParts?: AssistantRenderPart[];
  activeToolCalls?: ToolCallDisplay[];
  streamingThinking?: string;
}

export const TRANSIENT_CHAT_MESSAGE_ID = "__transient__";

export function parseToolCallIdsFromDataset(raw: string | undefined): string[] {
  return raw?.split(",").map((id) => id.trim()).filter(Boolean) ?? [];
}

export function parseMessageCopyTargetFromElement(element: Element): {
  hitMessageId: string | null;
  copyTarget: MessageCopyTarget;
} {
  const messageEl = element.closest("[data-chat-message-id]") as HTMLElement | null;
  const hitMessageId = messageEl?.dataset.chatMessageId?.trim() || null;

  const renderPartEl = element.closest("[data-render-part-kind]") as HTMLElement | null;
  const renderPartKind = renderPartEl?.dataset.renderPartKind;
  const renderPartKey = renderPartEl?.dataset.renderPartKey?.trim() ?? "";
  const scope = renderPartEl?.dataset.renderPartScope === "transient" ? "transient" : "history";

  if (renderPartKind === "thinking" && renderPartKey) {
    return {
      hitMessageId,
      copyTarget: { kind: "thinking", renderPartKey, scope },
    };
  }

  const toolLayoutEl = (
    element.closest("[data-tool-layout-kind='block']")
    ?? element.closest("[data-render-part-kind='toolCall']")
    ?? element.closest("[data-tool-layout-kind='group']")
    ?? renderPartEl
  ) as HTMLElement | null;

  if (renderPartKind === "toolCall" || toolLayoutEl) {
    const toolCallIds = parseToolCallIdsFromDataset(toolLayoutEl?.dataset.toolLayoutToolCallIds);
    if (toolCallIds.length > 0) {
      return {
        hitMessageId,
        copyTarget: {
          kind: "toolCall",
          toolCallIds,
          scope: toolLayoutEl?.dataset.renderPartScope === "transient"
            || toolLayoutEl?.dataset.toolLayoutScope === "transient"
            ? "transient"
            : scope,
        },
      };
    }
  }

  return { hitMessageId, copyTarget: { kind: "message" } };
}

function buildToolOutputMaps(messages: ChatMessage[]) {
  const output: Record<string, string> = {};
  const images: Record<string, ImageAttachment[]> = {};
  for (const message of messages) {
    if (message.role !== "tool" || !message.toolCallId) continue;
    output[message.toolCallId] = message.content;
    if (message.images?.length) {
      images[message.toolCallId] = message.images;
    }
  }
  return { output, images };
}

function toolCallInfosFromMessage(message: ChatMessage): ToolCallInfo[] {
  if (message.toolCalls?.length) return message.toolCalls;
  return message.renderParts
    ?.filter((part): part is Extract<AssistantRenderPart, { kind: "toolCall" }> => part.kind === "toolCall")
    .map((part) => part.toolCall) ?? [];
}

function formatIndentedBlock(text: string, indent: string): string {
  return text
    .split("\n")
    .map((line) => `${indent}${line}`)
    .join("\n");
}

export function formatToolCallForCopy(toolCall: ToolCallDisplay, depth = 0): string {
  const indent = "  ".repeat(depth);
  const lines: string[] = [
    `${indent}Tool: ${toolCall.name}`,
    `${indent}Status: ${toolCall.status}`,
  ];

  if (toolCall.arguments.trim()) {
    lines.push(`${indent}Arguments:`);
    lines.push(formatIndentedBlock(toolCall.arguments.trim(), `${indent}  `));
  }

  if (toolCall.output?.trim()) {
    lines.push(`${indent}Output:`);
    lines.push(formatIndentedBlock(toolCall.output.trim(), `${indent}  `));
  } else if (toolCall.status === "running") {
    lines.push(`${indent}Output: (running)`);
  }

  if (toolCall.images?.length) {
    lines.push(`${indent}[${toolCall.images.length} image(s) attached]`);
  }

  for (const nestedToolCall of toolCall.nestedToolCalls ?? []) {
    lines.push("");
    lines.push(formatToolCallForCopy(nestedToolCall, depth + 1));
  }

  return lines.join("\n");
}

function resolveHistoryThinkingCopyText(
  renderPartKey: string,
  context: ChatCopyContext,
  message: ChatMessage | null,
): string {
  const separatorIndex = renderPartKey.indexOf(":");
  if (separatorIndex <= 0) return message?.thinkingContent?.trim() ?? "";

  const itemId = renderPartKey.slice(0, separatorIndex);
  const partId = renderPartKey.slice(separatorIndex + 1);
  const targetMessage = context.messages.find((entry) => entry.id === itemId) ?? message;
  if (!targetMessage) return "";

  const renderPart = targetMessage.renderParts?.find(
    (part): part is Extract<AssistantRenderPart, { kind: "thinking" }> =>
      part.kind === "thinking" && part.id === partId,
  );
  if (renderPart?.content.trim()) return renderPart.content;

  return targetMessage.thinkingContent?.trim() ?? "";
}

function resolveTransientThinkingCopyText(
  renderPartKey: string,
  context: ChatCopyContext,
): string {
  const matchedPart = context.liveRenderParts?.find(
    (part): part is Extract<AssistantRenderPart, { kind: "thinking" }> =>
      part.kind === "thinking" && renderPartKey === `transient:${part.id}`,
  );
  if (matchedPart?.content.trim()) return matchedPart.content;
  return context.streamingThinking?.trim() ?? "";
}

export function resolveThinkingCopyText(
  target: Extract<MessageCopyTarget, { kind: "thinking" }>,
  context: ChatCopyContext,
  message: ChatMessage | null,
): string {
  if (target.scope === "transient") {
    return resolveTransientThinkingCopyText(target.renderPartKey, context);
  }
  return resolveHistoryThinkingCopyText(target.renderPartKey, context, message);
}

function resolveHistoryToolCallsCopyText(
  toolCallIds: string[],
  context: ChatCopyContext,
): ToolCallDisplay[] {
  const maps = buildToolOutputMaps(context.messages);
  const resolved = new Map<string, ToolCallDisplay>();

  for (const message of context.messages) {
    for (const toolCallInfo of toolCallInfosFromMessage(message)) {
      if (!toolCallIds.includes(toolCallInfo.id) || resolved.has(toolCallInfo.id)) continue;
      resolved.set(
        toolCallInfo.id,
        buildMessageToolCall(toolCallInfo, maps.output, maps.images),
      );
    }
  }

  return toolCallIds
    .map((id) => resolved.get(id))
    .filter((toolCall): toolCall is ToolCallDisplay => !!toolCall);
}

function resolveTransientToolCallsCopyText(
  toolCallIds: string[],
  context: ChatCopyContext,
): ToolCallDisplay[] {
  const activeById = new Map((context.activeToolCalls ?? []).map((toolCall) => [toolCall.id, toolCall]));
  return toolCallIds
    .map((id) => activeById.get(id))
    .filter((toolCall): toolCall is ToolCallDisplay => !!toolCall);
}

export function resolveToolCallsCopyText(
  target: Extract<MessageCopyTarget, { kind: "toolCall" }>,
  context: ChatCopyContext,
): string {
  const toolCalls = target.scope === "transient"
    ? resolveTransientToolCallsCopyText(target.toolCallIds, context)
    : resolveHistoryToolCallsCopyText(target.toolCallIds, context);

  if (toolCalls.length === 0) return "";
  return toolCalls.map((toolCall) => formatToolCallForCopy(toolCall)).join("\n\n");
}

export function buildChatMessageClipboardPayloadWithTarget(
  message: ChatMessage | null,
  copyTarget: MessageCopyTarget,
  context: ChatCopyContext,
): ChatMessageClipboardPayload {
  if (copyTarget.kind === "message") {
    if (!message) {
      return { text: "", draft: null, serializedDraft: null };
    }
    return buildChatMessageClipboardPayload(message);
  }

  if (copyTarget.kind === "thinking") {
    return {
      text: resolveThinkingCopyText(copyTarget, context, message),
      draft: null,
      serializedDraft: null,
    };
  }

  return {
    text: resolveToolCallsCopyText(copyTarget, context),
    draft: null,
    serializedDraft: null,
  };
}

export function canCopyMessageContextTarget(
  copyTarget: MessageCopyTarget | undefined,
  message: ChatMessage | null,
): boolean {
  if (copyTarget?.kind === "thinking" || copyTarget?.kind === "toolCall") {
    return true;
  }
  return !!message && message.role !== "tool";
}
