import { beforeEach, describe, expect, it, vi } from "vitest";

const ipcInvokeMock = vi.hoisted(() => vi.fn());

vi.mock("../services/ipc", () => ({
  ipcInvoke: ipcInvokeMock,
}));

import {
  applyMemoryProposal,
  ignoreMemoryProposal,
  memoryCreate,
  memoryList,
  memoryRetrieve,
  staleMemoryProposals,
} from "../services/memory";

describe("memory service", () => {
  beforeEach(() => {
    ipcInvokeMock.mockReset();
    ipcInvokeMock.mockResolvedValue(undefined);
  });

  it("lists entries with working directory filters", async () => {
    ipcInvokeMock.mockResolvedValueOnce([]);
    await memoryList({
      workingDir: "G:/Proj",
      category: "user",
      scope: "project",
      tags: ["prefs"],
      query: "theme",
      limit: 20,
      offset: 0,
    });

    expect(ipcInvokeMock).toHaveBeenCalledWith("memory_list", {
      workingDir: "G:/Proj",
      category: "user",
      scope: "project",
      tags: ["prefs"],
      query: "theme",
      limit: 20,
      offset: 0,
    });
  });

  it("creates entries with optional scope and tags", async () => {
    ipcInvokeMock.mockResolvedValueOnce({ id: "m1" });
    await memoryCreate({
      workingDir: "G:/Proj",
      category: "feedback",
      content: "Avoid renaming public APIs without impact review.",
      tags: ["api"],
      pinned: true,
      pinWeight: 120,
      sourceSessionId: "s1",
    });

    expect(ipcInvokeMock).toHaveBeenCalledWith("memory_create", {
      workingDir: "G:/Proj",
      category: "feedback",
      scope: null,
      content: "Avoid renaming public APIs without impact review.",
      tags: ["api"],
      pinned: true,
      pinWeight: 120,
      sourceSessionId: "s1",
    });
  });

  it("retrieves scored hits for prompt injection preview", async () => {
    ipcInvokeMock.mockResolvedValueOnce([]);
    await memoryRetrieve("G:/Proj", "unity scene graph", {
      limit: 8,
      tokenBudget: 600,
      scopes: ["project", "user"],
    });

    expect(ipcInvokeMock).toHaveBeenCalledWith("memory_retrieve", {
      workingDir: "G:/Proj",
      query: "unity scene graph",
      limit: 8,
      tokenBudget: 600,
      scopes: ["project", "user"],
    });
  });

  it("applies memory proposals via session id and proposal id", async () => {
    await applyMemoryProposal("s1", "mp_1");
    expect(ipcInvokeMock).toHaveBeenCalledWith("apply_memory_proposal", {
      sessionId: "s1",
      proposalId: "mp_1",
    });
  });

  it("stales and ignores pending memory proposals", async () => {
    await staleMemoryProposals("s1");
    await ignoreMemoryProposal("s1", "mp_2");

    expect(ipcInvokeMock).toHaveBeenNthCalledWith(1, "stale_memory_proposals", { sessionId: "s1" });
    expect(ipcInvokeMock).toHaveBeenNthCalledWith(2, "ignore_memory_proposal", {
      sessionId: "s1",
      proposalId: "mp_2",
    });
  });
});
