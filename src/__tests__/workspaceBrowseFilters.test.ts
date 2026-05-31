import { describe, expect, it } from "vitest";
import {
  normalizeWorkspaceBrowseFilters,
  workspaceBrowseFiltersActive,
} from "../composables/useWorkspaceBrowseFilters";

describe("workspaceBrowseFilters", () => {
  it("normalizes folder paths, file names, and extensions", () => {
    expect(
      normalizeWorkspaceBrowseFilters({
        blockedFolderNames: [" Assets/Generated ", "temp", "temp"],
        blockedFileNames: ["Thumbs.db"],
        blockedExtensions: ["DLL", ".bak"],
      }),
    ).toEqual({
      blockedFolderNames: ["Assets/Generated", "temp"],
      blockedFileNames: ["Thumbs.db"],
      blockedExtensions: [".dll", ".bak"],
    });
  });

  it("reports inactive filters when all lists are empty", () => {
    expect(workspaceBrowseFiltersActive(normalizeWorkspaceBrowseFilters({}))).toBe(false);
    expect(
      workspaceBrowseFiltersActive(
        normalizeWorkspaceBrowseFilters({ blockedExtensions: [".log"] }),
      ),
    ).toBe(true);
  });
});
