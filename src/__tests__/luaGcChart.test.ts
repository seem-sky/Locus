import { describe, expect, it } from "vitest";
import { buildLuaGcChartModel } from "../composables/luaGcChart";
import type { LuaGcSample } from "../services/luaGcMonitor";

function sample(
  memoryKb: number,
  gcDebtKb: number,
  allocKbSinceLast: number,
  index: number,
): LuaGcSample {
  return {
    sessionId: "session-1",
    frame: index,
    timeMs: index * 100,
    runtime: "xlua",
    memoryKb,
    gcDebtKb,
    gcStepMult: 200,
    gcRunning: false,
    gcPhase: "propagate",
    allocKbSinceLast,
    luaVersion: "Lua 5.4",
  };
}

describe("buildLuaGcChartModel", () => {
  it("returns empty ranges when there are no samples", () => {
    const model = buildLuaGcChartModel([]);
    expect(model.memory).toEqual([]);
    expect(model.debt).toEqual([]);
    expect(model.alloc).toEqual([]);
    expect(model.phaseBands).toEqual([]);
    expect(model.minY).toBe(0);
    expect(model.maxY).toBe(1);
  });

  it("maps each series to index-based points and computes y bounds", () => {
    const model = buildLuaGcChartModel([
      sample(1000, 50, 10, 0),
      sample(1100, 80, 120, 1),
      sample(1050, 20, 5, 2),
    ]);

    expect(model.memory).toEqual([
      { x: 0, y: 1000 },
      { x: 1, y: 1100 },
      { x: 2, y: 1050 },
    ]);
    expect(model.debt).toEqual([
      { x: 0, y: 50 },
      { x: 1, y: 80 },
      { x: 2, y: 20 },
    ]);
    expect(model.alloc).toEqual([
      { x: 0, y: 10 },
      { x: 1, y: 120 },
      { x: 2, y: 5 },
    ]);
    expect(model.minY).toBe(5);
    expect(model.maxY).toBe(1100);
  });

  it("expands maxY when all values are equal", () => {
    const model = buildLuaGcChartModel([
      sample(500, 500, 500, 0),
      sample(500, 500, 500, 1),
    ]);
    expect(model.maxY).toBeGreaterThan(model.minY);
  });

  it("groups consecutive gc phases into background bands", () => {
    const model = buildLuaGcChartModel([
      { ...sample(1000, 50, 10, 0), gcPhase: "pause" },
      { ...sample(1010, 50, 10, 1), gcPhase: "pause" },
      { ...sample(1020, 50, 10, 2), gcPhase: "atomic" },
      { ...sample(1030, 50, 10, 3), gcPhase: "sweep" },
    ]);

    expect(model.phaseBands).toEqual([
      { startIndex: 0, endIndex: 1, phase: "pause" },
      { startIndex: 2, endIndex: 2, phase: "atomic" },
      { startIndex: 3, endIndex: 3, phase: "sweep" },
    ]);
  });
});
