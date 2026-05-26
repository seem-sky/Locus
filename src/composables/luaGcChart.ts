import type { LuaGcSample } from "../services/luaGcMonitor";

export interface ChartSeriesPoint {
  x: number;
  y: number;
}

export interface LuaGcPhaseBand {
  startIndex: number;
  endIndex: number;
  phase: string;
}

export interface LuaGcChartModel {
  memory: ChartSeriesPoint[];
  debt: ChartSeriesPoint[];
  alloc: ChartSeriesPoint[];
  phaseBands: LuaGcPhaseBand[];
  minY: number;
  maxY: number;
}

export function buildLuaGcChartModel(samples: LuaGcSample[]): LuaGcChartModel {
  if (samples.length === 0) {
    return { memory: [], debt: [], alloc: [], phaseBands: [], minY: 0, maxY: 1 };
  }

  const memory: ChartSeriesPoint[] = [];
  const debt: ChartSeriesPoint[] = [];
  const alloc: ChartSeriesPoint[] = [];
  const phaseBands = buildPhaseBands(samples);
  let minY = Number.POSITIVE_INFINITY;
  let maxY = 0;

  samples.forEach((sample, index) => {
    const x = index;
    memory.push({ x, y: sample.memoryKb });
    debt.push({ x, y: sample.gcDebtKb });
    alloc.push({ x, y: sample.allocKbSinceLast });
    minY = Math.min(minY, sample.memoryKb, sample.gcDebtKb, sample.allocKbSinceLast);
    maxY = Math.max(maxY, sample.memoryKb, sample.gcDebtKb, sample.allocKbSinceLast);
  });

  if (!Number.isFinite(minY)) minY = 0;
  if (maxY <= minY) maxY = minY + 1;

  return { memory, debt, alloc, phaseBands, minY, maxY };
}

function buildPhaseBands(samples: LuaGcSample[]): LuaGcPhaseBand[] {
  if (samples.length === 0) return [];

  const bands: LuaGcPhaseBand[] = [];
  let startIndex = 0;
  let currentPhase = normalizeGcPhase(samples[0].gcPhase);

  for (let index = 1; index < samples.length; index += 1) {
    const phase = normalizeGcPhase(samples[index].gcPhase);
    if (phase === currentPhase) continue;
    bands.push({ startIndex, endIndex: index - 1, phase: currentPhase });
    startIndex = index;
    currentPhase = phase;
  }

  bands.push({
    startIndex,
    endIndex: samples.length - 1,
    phase: currentPhase,
  });
  return bands;
}

function normalizeGcPhase(phase: string): string {
  const normalized = phase.trim().toLowerCase();
  if (!normalized || normalized === "unknown" || normalized === "incremental") {
    return "propagate";
  }
  return normalized;
}

export function drawLuaGcChart(
  canvas: HTMLCanvasElement,
  model: LuaGcChartModel,
  options?: {
    showMemory?: boolean;
    showDebt?: boolean;
    showAlloc?: boolean;
  },
): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const width = canvas.width;
  const height = canvas.height;
  const padding = { top: 12, right: 12, bottom: 22, left: 48 };
  const plotW = Math.max(1, width - padding.left - padding.right);
  const plotH = Math.max(1, height - padding.top - padding.bottom);

  ctx.clearRect(0, 0, width, height);

  const styles = getComputedStyle(canvas);
  const border = styles.getPropertyValue("--border-color").trim() || "#334155";
  const text = styles.getPropertyValue("--text-secondary").trim() || "#94a3b8";
  const memoryColor = styles.getPropertyValue("--accent-color").trim() || "#3b82f6";
  const debtColor = "#f59e0b";
  const allocColor = "#22c55e";

  ctx.fillStyle = colorMix(styles, "--panel-bg", 0.92);
  ctx.fillRect(0, 0, width, height);

  ctx.strokeStyle = border;
  ctx.lineWidth = 1;
  ctx.strokeRect(padding.left, padding.top, plotW, plotH);

  const maxX = Math.max(
    model.memory.length,
    model.debt.length,
    model.alloc.length,
    1,
  ) - 1;
  const rangeY = model.maxY - model.minY;

  const toX = (x: number) => padding.left + (maxX <= 0 ? 0 : (x / maxX) * plotW);
  const toY = (y: number) =>
    padding.top + plotH - ((y - model.minY) / rangeY) * plotH;

  drawPhaseBands(ctx, model.phaseBands, padding, plotW, plotH, maxX, toX);

  ctx.fillStyle = text;
  ctx.font = "11px system-ui, sans-serif";
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";
  for (let i = 0; i <= 4; i += 1) {
    const value = model.minY + (rangeY * i) / 4;
    const y = toY(value);
    ctx.strokeStyle = colorMix(styles, "--border-color", 0.35);
    ctx.beginPath();
    ctx.moveTo(padding.left, y);
    ctx.lineTo(padding.left + plotW, y);
    ctx.stroke();
    ctx.fillStyle = text;
    ctx.fillText(formatKb(value), padding.left - 6, y);
  }

  const showMemory = options?.showMemory !== false;
  const showDebt = options?.showDebt !== false;
  const showAlloc = options?.showAlloc !== false;

  if (showMemory) drawSeries(ctx, model.memory, memoryColor, toX, toY);
  if (showDebt) drawSeries(ctx, model.debt, debtColor, toX, toY);
  if (showAlloc) drawSeries(ctx, model.alloc, allocColor, toX, toY);
}

function drawPhaseBands(
  ctx: CanvasRenderingContext2D,
  bands: LuaGcPhaseBand[],
  padding: { top: number; right: number; bottom: number; left: number },
  plotW: number,
  plotH: number,
  maxX: number,
  toX: (x: number) => number,
): void {
  if (bands.length === 0 || maxX < 0) return;

  const bandEndX = (endIndex: number) => {
    const next = endIndex + 1;
    return next > maxX ? padding.left + plotW : toX(next);
  };

  for (const band of bands) {
    const x0 = toX(band.startIndex);
    const x1 = bandEndX(band.endIndex);
    const width = Math.max(1, x1 - x0);
    ctx.fillStyle = phaseBandColor(band.phase);
    ctx.fillRect(x0, padding.top, width, plotH);
  }
}

function phaseBandColor(phase: string): string {
  switch (phase) {
    case "pause":
      return "rgba(148, 163, 184, 0.14)";
    case "propagate":
      return "rgba(59, 130, 246, 0.12)";
    case "atomic":
      return "rgba(239, 68, 68, 0.14)";
    case "sweep":
      return "rgba(34, 197, 94, 0.12)";
    default:
      return "rgba(148, 163, 184, 0.08)";
  }
}

function drawSeries(
  ctx: CanvasRenderingContext2D,
  points: ChartSeriesPoint[],
  color: string,
  toX: (x: number) => number,
  toY: (y: number) => number,
): void {
  if (points.length === 0) return;
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  points.forEach((point, index) => {
    const x = toX(point.x);
    const y = toY(point.y);
    if (index === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
}

function formatKb(value: number): string {
  if (value >= 1024) return `${(value / 1024).toFixed(1)} MB`;
  return `${value.toFixed(0)} KB`;
}

function colorMix(styles: CSSStyleDeclaration, variable: string, alpha: number): string {
  const raw = styles.getPropertyValue(variable).trim();
  if (!raw) return `rgba(15, 23, 42, ${alpha})`;
  return `color-mix(in srgb, ${raw} ${Math.round(alpha * 100)}%, transparent)`;
}
