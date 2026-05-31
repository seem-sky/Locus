import { describe, expect, it } from "vitest";

import type { TokenUsage } from "../types";
import {
  buildTokenUsageMetrics,
  formatTokenCount,
  hasSessionTokenUsage,
  sessionInputTokenTotal,
  shouldShowTokenUsageBar,
} from "../components/chat/tokenUsageDisplay";

function makeUsage(overrides: Partial<TokenUsage> = {}): TokenUsage {
  return {
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalCacheReadTokens: 0,
    totalCacheWriteTokens: 0,
    totalCostUsd: 0,
    pricedRounds: 0,
    contextTokens: 0,
    contextLimit: 0,
    ...overrides,
  };
}

describe("tokenUsageDisplay", () => {
  it("shows input and output when no cache usage exists", () => {
    const metrics = buildTokenUsageMetrics(makeUsage({
      totalInputTokens: 1200,
      totalOutputTokens: 480,
    }));

    expect(metrics).toEqual([
      {
        key: "input",
        shortLabel: "input",
        tooltipLabel: "input",
        value: 1200,
      },
      {
        key: "output",
        shortLabel: "output",
        tooltipLabel: "output",
        value: 480,
      },
    ]);
  });

  it("shows uncached, cache write, cache read, and output when cache usage exists", () => {
    const metrics = buildTokenUsageMetrics(makeUsage({
      totalInputTokens: 900,
      totalOutputTokens: 240,
      totalCacheReadTokens: 3200,
      totalCacheWriteTokens: 600,
    }));

    expect(metrics).toEqual([
      {
        key: "uncached-input",
        shortLabel: "uncached",
        tooltipLabel: "uncached input",
        value: 900,
      },
      {
        key: "cached-input-write",
        shortLabel: "cache write",
        tooltipLabel: "cached input write",
        value: 600,
      },
      {
        key: "cached-input-read",
        shortLabel: "cache read",
        tooltipLabel: "cached input read",
        value: 3200,
      },
      {
        key: "output",
        shortLabel: "output",
        tooltipLabel: "output",
        value: 240,
      },
    ]);
  });

  it("keeps both cache categories visible when only one cache bucket is non-zero", () => {
    const metrics = buildTokenUsageMetrics(makeUsage({
      totalInputTokens: 900,
      totalOutputTokens: 240,
      totalCacheReadTokens: 3200,
      totalCacheWriteTokens: 0,
    }));

    expect(metrics.map((metric) => metric.key)).toEqual([
      "uncached-input",
      "cached-input-write",
      "cached-input-read",
      "output",
    ]);
    expect(metrics[1]?.value).toBe(0);
  });

  it("aggregates session input across cache buckets", () => {
    const usage = makeUsage({
      totalInputTokens: 900,
      totalCacheReadTokens: 3200,
      totalCacheWriteTokens: 600,
      totalOutputTokens: 240,
    });
    expect(sessionInputTokenTotal(usage)).toBe(4700);
    expect(hasSessionTokenUsage(usage)).toBe(true);
    expect(shouldShowTokenUsageBar(usage)).toBe(true);
    expect(formatTokenCount(4700)).toBe("4.7k");
  });

  it("shows the bar for context window usage without session totals", () => {
    expect(shouldShowTokenUsageBar(makeUsage({
      contextTokens: 1200,
      contextLimit: 128000,
    }))).toBe(true);
    expect(hasSessionTokenUsage(makeUsage())).toBe(false);
  });
});
