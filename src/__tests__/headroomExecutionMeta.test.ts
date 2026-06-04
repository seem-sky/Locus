import { describe, expect, it } from "vitest";
import {
  parseHeadroomExecutionMeta,
  parseHeadroomProgressInfo,
  resolveHeadroomDisplayForToolCall,
} from "../composables/headroomExecutionMeta";

describe("headroomExecutionMeta", () => {
  it("returns rewritten status when command changed", () => {
    const parsed = parseHeadroomExecutionMeta({
      headroom: {
        rewrite: {
          enabled: true,
          available: true,
          rewritten: true,
          originalCommand: "git status",
          executedCommand: "rtk git status",
        },
      },
    });
    expect(parsed).toEqual({
      rewriteStatus: "rewritten",
      originalCommand: "git status",
      executedCommand: "rtk git status",
      compress: null,
    });
  });

  it("accepts legacy rtk meta shape", () => {
    const parsed = parseHeadroomExecutionMeta({
      rtk: {
        enabled: true,
        available: true,
        rewritten: false,
        originalCommand: "echo ok",
        executedCommand: "echo ok",
      },
    });
    expect(parsed?.rewriteStatus).toBe("passthrough");
    expect(parsed?.originalCommand).toBe("echo ok");
  });

  it("reads compress stats from headroom meta", () => {
    const parsed = parseHeadroomExecutionMeta({
      headroom: {
        rewrite: {
          enabled: true,
          available: true,
          rewritten: true,
          originalCommand: "cargo test",
          executedCommand: "rtk cargo test",
        },
        compress: {
          enabled: true,
          available: true,
          compressed: true,
          originalChars: 5000,
          compressedChars: 800,
          tokensSaved: 1200,
        },
      },
    });
    expect(parsed?.compress?.tokensSaved).toBe(1200);
  });

  it("reads Headroom progress payload while bash is running", () => {
    const parsed = parseHeadroomProgressInfo(JSON.stringify({
      enabled: true,
      available: true,
      rewritten: true,
      originalCommand: "git status",
      executedCommand: "rtk git status",
    }));
    expect(parsed?.rewriteStatus).toBe("rewritten");
  });

  it("prefers execution meta over progress payload", () => {
    const parsed = resolveHeadroomDisplayForToolCall({
      name: "bash",
      executionMeta: {
        headroom: {
          rewrite: {
            enabled: true,
            available: false,
            rewritten: false,
            originalCommand: "git status",
          },
        },
      },
      progress: {
        title: "Headroom",
        info: JSON.stringify({
          enabled: true,
          available: true,
          rewritten: true,
          originalCommand: "git status",
          executedCommand: "rtk git status",
        }),
        state: "headroom",
      },
    });
    expect(parsed?.rewriteStatus).toBe("unavailable");
  });

  it("returns null for non-bash tools without meta", () => {
    expect(resolveHeadroomDisplayForToolCall({ name: "read" })).toBeNull();
  });
});
