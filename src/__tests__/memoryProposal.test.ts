import { describe, expect, it } from "vitest";
import { synthesizeLegacyRenderParts } from "../composables/assistantRenderParts";
import type { ChatMessage } from "../types";

describe("assistantRenderParts memory proposals", () => {
  it("includes memory proposal parts for standalone proposal messages", () => {
    const message: ChatMessage = {
      id: "mp-msg",
      role: "assistant",
      content: "",
      createdAt: 1,
      memoryProposal: {
        proposalId: "mp_1",
        status: "pending",
        confidence: 0.9,
        verify: "none",
        estTokens: 12,
        items: [
          {
            category: "user",
            content: "Prefer concise Chinese replies.",
            tags: ["locale"],
            scope: "user",
          },
        ],
        createdAt: 1,
        updatedAt: 1,
      },
    };

    const parts = synthesizeLegacyRenderParts(message);
    expect(parts.some((part) => part.kind === "memoryProposal")).toBe(true);
    const proposalPart = parts.find((part) => part.kind === "memoryProposal");
    expect(proposalPart?.kind === "memoryProposal" && proposalPart.message.memoryProposal?.proposalId).toBe("mp_1");
  });
});
