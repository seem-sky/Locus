import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const cwd = process.cwd();

function read(relPath: string) {
  return readFileSync(resolve(cwd, relPath), "utf8");
}

describe("main window state persistence", () => {
  it("registers window-state plugin for the main window only", () => {
    const lib = read("src-tauri/src/lib.rs");
    const cargo = read("src-tauri/Cargo.toml");

    expect(cargo).toContain('tauri-plugin-window-state = "2.4.1"');
    expect(lib).toContain("tauri_plugin_window_state::Builder::new()");
    expect(lib).toContain("StateFlags::SIZE");
    expect(lib).toContain("StateFlags::POSITION");
    expect(lib).toContain("StateFlags::MAXIMIZED");
    expect(lib).toContain(".with_filter(|label| label == MAIN_WINDOW_LABEL)");
    expect(lib).not.toContain("StateFlags::VISIBLE");
  });
});
