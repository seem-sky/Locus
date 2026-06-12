import type { TokenUsage } from "../../types";

export interface TokenUsageMetric {
  key: "input" | "uncached-input" | "cached-input-write" | "cached-input-read" | "output";
  shortLabel: string;
  tooltipLabel: string;
  value: number;
}

export function formatTokenCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return n.toString();
}

export function sessionInputTokenTotal(usage: TokenUsage): number {
  return usage.totalInputTokens + usage.totalCacheReadTokens + usage.totalCacheWriteTokens;
}

export function hasSessionTokenUsage(usage: TokenUsage): boolean {
  return sessionInputTokenTotal(usage) > 0 || usage.totalOutputTokens > 0;
}

export function hasContextWindowUsage(usage: TokenUsage): boolean {
  return usage.contextTokens > 0 && usage.contextLimit > 0;
}

export function shouldShowTokenUsageBar(usage: TokenUsage): boolean {
  return hasSessionTokenUsage(usage) || hasContextWindowUsage(usage);
}

export function metricI18nKey(key: TokenUsageMetric["key"]): string {
  switch (key) {
    case "input":
      return "chat.tokenUsage.metric.input";
    case "uncached-input":
      return "chat.tokenUsage.metric.uncached";
    case "cached-input-write":
      return "chat.tokenUsage.metric.cacheWrite";
    case "cached-input-read":
      return "chat.tokenUsage.metric.cacheRead";
    case "output":
      return "chat.tokenUsage.metric.output";
  }
}

export function shouldShowTokenUsageMetric(metric: TokenUsageMetric): boolean {
  if (metric.value > 0) return true;
  return metric.key === "cached-input-write" || metric.key === "cached-input-read";
}

export function visibleTokenUsageMetrics(usage: TokenUsage): TokenUsageMetric[] {
  return buildTokenUsageMetrics(usage).filter(shouldShowTokenUsageMetric);
}

export function buildTokenUsageMetrics(usage: TokenUsage): TokenUsageMetric[] {
  const hasCache = usage.totalCacheReadTokens > 0 || usage.totalCacheWriteTokens > 0;

  if (!hasCache) {
    return [
      {
        key: "input",
        shortLabel: "input",
        tooltipLabel: "input",
        value: usage.totalInputTokens,
      },
      {
        key: "output",
        shortLabel: "output",
        tooltipLabel: "output",
        value: usage.totalOutputTokens,
      },
    ];
  }

  return [
    {
      key: "uncached-input",
      shortLabel: "uncached",
      tooltipLabel: "uncached input",
      value: usage.totalInputTokens,
    },
    {
      key: "cached-input-write",
      shortLabel: "cache write",
      tooltipLabel: "cached input write",
      value: usage.totalCacheWriteTokens,
    },
    {
      key: "cached-input-read",
      shortLabel: "cache read",
      tooltipLabel: "cached input read",
      value: usage.totalCacheReadTokens,
    },
    {
      key: "output",
      shortLabel: "output",
      tooltipLabel: "output",
      value: usage.totalOutputTokens,
    },
  ];
}
