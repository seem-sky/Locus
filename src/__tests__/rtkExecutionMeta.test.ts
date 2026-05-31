import { describe, expect, it } from "vitest";
import {
  parseRtkExecutionMeta,
  parseRtkProgressInfo,
  resolveRtkDisplayForToolCall,
} from "../composables/rtkExecutionMeta";

describe("rtkExecutionMeta", () => {
  it("returns rewritten status when command changed", () => {
    const parsed = parseRtkExecutionMeta({
      rtk: {
        enabled: true,
        available: true,
        rewritten: true,
        originalCommand: "git status",
        executedCommand: "rtk git status",
      },
    });
    expect(parsed).toEqual({
      status: "rewritten",
      originalCommand: "git status",
      executedCommand: "rtk git status",
    });
  });

  it("accepts snake_case fields from backend", () => {
    const parsed = parseRtkExecutionMeta({
      rtk: {
        enabled: true,
        available: true,
        rewritten: false,
        original_command: "echo ok",
        executed_command: "echo ok",
      },
    });
    expect(parsed?.status).toBe("passthrough");
    expect(parsed?.originalCommand).toBe("echo ok");
  });

  it("reads RTK progress payload while bash is running", () => {
    const parsed = parseRtkProgressInfo(JSON.stringify({
      enabled: true,
      available: true,
      rewritten: true,
      originalCommand: "git status",
      executedCommand: "rtk git status",
    }));
    expect(parsed?.status).toBe("rewritten");
  });

  it("prefers execution meta over progress payload", () => {
    const parsed = resolveRtkDisplayForToolCall({
      name: "bash",
      executionMeta: {
        rtk: {
          enabled: true,
          available: false,
          rewritten: false,
          originalCommand: "git status",
        },
      },
      progress: {
        title: "RTK",
        info: JSON.stringify({
          enabled: true,
          available: true,
          rewritten: true,
          originalCommand: "git status",
          executedCommand: "rtk git status",
        }),
        state: "rtk",
      },
    });
    expect(parsed?.status).toBe("unavailable");
  });

  it("returns null for non-bash tools without meta", () => {
    expect(resolveRtkDisplayForToolCall({ name: "read" })).toBeNull();
  });

  it("detects bash through tool_call arguments", () => {
    expect(resolveRtkDisplayForToolCall({
      name: "tool_call",
      arguments: JSON.stringify({ toolName: "bash", arguments: { command: "git status" } }),
      progress: {
        title: "RTK",
        info: JSON.stringify({
          enabled: true,
          available: true,
          rewritten: true,
          originalCommand: "git status",
          executedCommand: "rtk git status",
        }),
        state: "rtk",
      },
    })?.status).toBe("rewritten");
  });
});
