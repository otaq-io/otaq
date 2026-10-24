import { describe, expect, it } from "vitest";

import type { Keymap } from "../generated/Keymap";
import type { KeyModifiers } from "../generated/KeyModifiers";
import {
  clampFontSize,
  findCount,
  fontSizeOf,
  jumpLabel,
  NEW_LINE,
  opaqueHex,
  rendererFor,
  takesInput,
  terminalKey,
  typingIn,
} from "./state";

const NONE: KeyModifiers = { control: false, alt: false, shift: false, platform: false, function: false };
const key = (
  key: string,
  code: string,
  mods: Partial<Record<"ctrlKey" | "shiftKey" | "metaKey" | "altKey", boolean>>,
) => ({
  key,
  code,
  ctrlKey: false,
  shiftKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
});

describe("the terminal's state", () => {
  it("takes input only while live: reconnecting and ended keep the scrollback read-only", () => {
    expect(takesInput("live")).toBe(true);
    for (const phase of ["attaching", "reconnecting", "ended", "no_session"] as const) {
      expect(takesInput(phase)).toBe(false);
    }
  });

  it("holds what is typed while attaching until live (d0126), and drops it while reconnecting and after the end", () => {
    expect(typingIn("live")).toBe("send");
    expect(typingIn("attaching")).toBe("hold");
    for (const phase of ["reconnecting", "ended", "no_session"] as const)
      expect(typingIn(phase)).toBe("drop");
  });

  it("counts find results the way the find bar shows them", () => {
    expect(findCount(2, 12)).toBe("3 of 12");
    expect(findCount(-1, 12)).toBe("12");
    expect(findCount(-1, 0)).toBe("No results");
    expect(findCount(4, 1000)).toBe("5 of 1000+");
  });

  it("labels the jump-to-latest pill", () => {
    expect(jumpLabel(0)).toBe("Jump to latest");
    expect(jumpLabel(1)).toBe("1 new line");
    expect(jumpLabel(1420)).toBe("1,420 new lines");
  });

  it("keeps the font size in 8–32 and a picked size only until the setting changes", () => {
    expect(clampFontSize(4)).toBe(8);
    expect(clampFontSize(40)).toBe(32);
    expect(fontSizeOf(undefined, undefined)).toBe(13);
    expect(fontSizeOf(15, undefined)).toBe(15);
    expect(fontSizeOf(15, { setting: 15, size: 17 })).toBe(17);
    expect(fontSizeOf(18, { setting: 15, size: 17 })).toBe(18);
  });

  it("starts the renderer the setting picks, and DOM once WebGL failed", () => {
    expect(rendererFor("webgl", false)).toBe("webgl");
    expect(rendererFor("webgl", true)).toBe("dom");
    expect(rendererFor("dom", false)).toBe("dom");
  });

  it("finds its own keys in the Rust map", () => {
    const map: Keymap = {
      platform: "win_linux",
      log: false,
      typing_hint: "Typing? Click where you want to type",
      letters: true,
      actions: [],
      sequences: [],
      bindings: [],
      terminal_app_modifiers: [],
      terminal_keys: [
        {
          key: "copy",
          keystroke: { key: "c", code: "KeyC", modifiers: { ...NONE, control: true, shift: true } },
        },
        {
          key: "font_bigger",
          keystroke: { key: "=", code: "Equal", modifiers: { ...NONE, control: true, shift: true } },
        },
      ],
    };
    expect(terminalKey(map, key("C", "KeyC", { ctrlKey: true, shiftKey: true }))).toBe("copy");
    // Ctrl+C alone is the program's (interrupt).
    expect(terminalKey(map, key("c", "KeyC", { ctrlKey: true }))).toBeUndefined();
    expect(terminalKey(map, key("+", "Equal", { ctrlKey: true, shiftKey: true }))).toBe("font_bigger");
    expect(terminalKey(undefined, key("C", "KeyC", { ctrlKey: true, shiftKey: true }))).toBeUndefined();
  });

  it("sends a new line, not Enter, on Shift+Enter", () => {
    const map: Keymap = {
      platform: "mac",
      log: false,
      typing_hint: "Typing? Click where you want to type",
      letters: true,
      actions: [],
      sequences: [],
      bindings: [],
      terminal_app_modifiers: [],
      terminal_keys: [
        {
          key: "new_line",
          keystroke: { key: "Enter", code: "Enter", modifiers: { ...NONE, shift: true } },
        },
      ],
    };
    expect(terminalKey(map, key("Enter", "Enter", { shiftKey: true }))).toBe("new_line");
    expect(NEW_LINE).toBe("\x1b\r");
    // Enter, ⌥↵ and ⌘↵ stay xterm.js's (Enter sends; ⌥↵ is already ESC + CR).
    expect(terminalKey(map, key("Enter", "Enter", {}))).toBeUndefined();
    expect(terminalKey(map, key("Enter", "Enter", { altKey: true }))).toBeUndefined();
    expect(terminalKey(map, key("Enter", "Enter", { metaKey: true, shiftKey: true }))).toBeUndefined();
  });

  it("drops the alpha of a theme colour for xterm.js's decorations", () => {
    expect(opaqueHex(" #4c57d633")).toBe("#4c57d6");
    expect(opaqueHex("#d4500a")).toBe("#d4500a");
  });
});
