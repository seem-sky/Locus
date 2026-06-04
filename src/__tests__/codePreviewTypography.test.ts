import { describe, expect, it } from "vitest";
import { clampCodePreviewTypography } from "../composables/useDisplaySettings";

describe("clampCodePreviewTypography", () => {
  it("clamps values to supported ranges", () => {
    expect(clampCodePreviewTypography({ fontSize: 32, lineHeight: 9, letterSpacing: 2 })).toEqual({
      fontSize: 24,
      lineHeight: 2.5,
      letterSpacing: 0.2,
    });
    expect(clampCodePreviewTypography({ fontSize: 6, lineHeight: 0.2, letterSpacing: -1 })).toEqual({
      fontSize: 10,
      lineHeight: 1,
      letterSpacing: -0.05,
    });
  });

  it("falls back to defaults for invalid input", () => {
    expect(clampCodePreviewTypography({ fontSize: NaN })).toEqual({
      fontSize: 12,
      lineHeight: 1.5,
      letterSpacing: 0,
    });
  });
});
