import { afterEach, describe, expect, it, vi } from "vitest";

import type { KeyAction } from "../generated/KeyAction";
import type { KeyBinding } from "../generated/KeyBinding";
import type { KeyPlace } from "../generated/KeyPlace";
import type { Keymap } from "../generated/Keymap";
import type { KeyModifiers } from "../generated/KeyModifiers";
import {
  BURST_MS,
  createBurst,
  type Dispatch,
  dispatchKey,
  type KeyContext,
  type KeyLike,
  inEditorText,
  inKeyRecorder,
  KEY_RECORDER,
  keyName,
  keysKeptBy,
  type LetterHold,
  lettersKeptBy,
  logLine,
  stopsEvent,
  widgetOpen,
} from "./keys";

// A few rows of the map as `keymap.map` sends them (the Rust tests cover the whole table).
const NONE: KeyModifiers = { control: false, alt: false, shift: false, platform: false, function: false };
const CMD = { ...NONE, platform: true };
const CTRL = { ...NONE, control: true };
const CTRL_SHIFT = { ...NONE, control: true, shift: true };

function bind(
  action: KeyAction,
  text: string,
  key: string,
  code: string,
  modifiers: KeyModifiers,
  place: KeyPlace = "outside_terminals",
) {
  return {
    action,
    slot: 0,
    text,
    key: { kind: "stroke", stroke: { key, code, modifiers } },
    place,
    custom: false,
  } satisfies KeyBinding;
}

const EVERYWHERE = "everywhere";
const INSIDE = "inside_terminals";

const MAC: Keymap = {
  platform: "mac",
  log: false,
  typing_hint: "Typing? Click where you want to type",
  letters: true,
  actions: [],
  sequences: [],
  terminal_keys: [],
  terminal_app_modifiers: [CMD],
  bindings: [
    bind({ action: "command_palette" }, "⌘K", "k", "KeyK", CMD, EVERYWHERE),
    bind({ action: "switch_project" }, "⌘P", "p", "KeyP", CMD, EVERYWHERE),
    bind({ action: "open_activity" }, "⇧⌘U", "u", "KeyU", { ...CMD, shift: true }, EVERYWHERE),
    bind({ action: "terminals" }, "⌃`", "`", "Backquote", CTRL, EVERYWHERE),
    bind({ action: "next_tab" }, "⌃Tab", "Tab", "Tab", CTRL, EVERYWHERE),
    bind({ action: "project", number: 1 }, "⌘1", "1", "Digit1", CMD, EVERYWHERE),
    bind({ action: "open_panel" }, "↵", "Enter", "Enter", NONE),
    bind({ action: "close_panel" }, "Esc", "Escape", "Escape", NONE),
    bind({ action: "send_back" }, "⌘⌫", "Backspace", "Backspace", CMD, EVERYWHERE),
    bind({ action: "find_in_terminal" }, "⌘F", "f", "KeyF", CMD, EVERYWHERE),
    {
      action: { action: "next_ticket" },
      slot: 1,
      text: "j",
      key: { kind: "letter", letter: "j" },
      place: "outside_terminals",
      custom: false,
    },
  ],
};

const WIN: Keymap = {
  platform: "win_linux",
  log: false,
  typing_hint: "Typing? Click where you want to type",
  letters: true,
  actions: [],
  sequences: [],
  terminal_keys: [],
  terminal_app_modifiers: [CTRL_SHIFT, { ...NONE, platform: true }],
  bindings: [
    bind({ action: "command_palette" }, "Ctrl+K", "k", "KeyK", CTRL, EVERYWHERE),
    bind({ action: "switch_project" }, "Ctrl+P", "p", "KeyP", CTRL, EVERYWHERE),
    bind({ action: "terminals" }, "Ctrl+`", "`", "Backquote", CTRL, EVERYWHERE),
    bind({ action: "new_ticket" }, "Ctrl+N", "n", "KeyN", CTRL),
    bind({ action: "new_ticket" }, "Ctrl+Shift+N", "n", "KeyN", CTRL_SHIFT, INSIDE),
    bind({ action: "find_in_terminal" }, "Ctrl+Shift+F", "f", "KeyF", CTRL_SHIFT, INSIDE),
  ],
};

function key(name: string, code: string, mods: Partial<Omit<KeyLike, "key" | "code">> = {}): KeyLike {
  return { key: name, code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods };
}

const BOARD: KeyContext = {
  inTerminal: false,
  focusKeeps: new Set(),
  modal: false,
  editorText: false,
  letters: "none",
  recording: false,
};
const TERMINAL: KeyContext = { ...BOARD, inTerminal: true };

/** A dispatcher whose screens handle `handled`; records what ran and whether the terminal was left. */
function setup(map: Keymap | undefined, handled: KeyAction["action"][] = []) {
  const ran: string[] = [];
  let left = 0;
  let waiting = false;
  const d: Dispatch = {
    keymap: () => map,
    run: (action) => {
      if (!handled.includes(action.action)) return false;
      ran.push("number" in action ? `${action.action} ${String(action.number)}` : action.action);
      return true;
    },
    leaveTerminal: () => left++,
    mac: map?.platform === "mac",
    waiting: () => waiting,
    wait: (on) => {
      waiting = on;
    },
  };
  return { d, ran, left: () => left, waiting: () => waiting };
}

describe("the key dispatcher on the board", () => {
  it("runs the handler of the bound action and stops the key", () => {
    const { d, ran } = setup(MAC, ["command_palette"]);
    const outcome = dispatchKey(d, key("k", "KeyK", { metaKey: true }), BOARD);
    expect(outcome).toEqual({ kind: "ran", action: { action: "command_palette" } });
    expect(stopsEvent(outcome)).toBe(true);
    expect(ran).toEqual(["command_palette"]);
  });

  it("matches letters by layout and digits by physical key", () => {
    const { d, ran } = setup(MAC, ["command_palette", "project"]);
    // Dvorak: the key that types k is where QWERTY has V.
    dispatchKey(d, key("k", "KeyV", { metaKey: true }), BOARD);
    // AZERTY types & on the 1 key.
    const project = dispatchKey(d, key("&", "Digit1", { metaKey: true }), BOARD);
    expect(project).toEqual({ kind: "ran", action: { action: "project", number: 1 } });
    // A Cyrillic layout types л on K's key.
    dispatchKey(d, key("л", "KeyK", { metaKey: true }), BOARD);
    expect(ran).toEqual(["command_palette", "project 1", "command_palette"]);
  });

  it("runs a single letter while nobody is typing", () => {
    const { d, ran } = setup(MAC, ["next_ticket"]);
    expect(dispatchKey(d, key("j", "KeyJ"), BOARD)).toEqual({
      kind: "ran",
      action: { action: "next_ticket" },
    });
    expect(ran).toEqual(["next_ticket"]);
  });

  it("needs the exact modifiers", () => {
    const { d, ran } = setup(MAC, ["command_palette", "open_activity"]);
    expect(dispatchKey(d, key("K", "KeyK", { metaKey: true, shiftKey: true }), BOARD).kind).toBe("passed");
    expect(dispatchKey(d, key("U", "KeyU", { metaKey: true, shiftKey: true }), BOARD).kind).toBe("ran");
    expect(dispatchKey(d, key("k", "KeyK", { ctrlKey: true }), BOARD).kind).toBe("passed");
    expect(ran).toEqual(["open_activity"]);
  });

  it("swallows a bound key nobody handles, so ⌘P never prints", () => {
    const { d } = setup(MAC);
    const outcome = dispatchKey(d, key("p", "KeyP", { metaKey: true }), BOARD);
    expect(outcome).toEqual({ kind: "unhandled", action: { action: "switch_project" }, swallowed: true });
    expect(stopsEvent(outcome)).toBe(true);
  });

  it("swallows the webview's own keys without acting", () => {
    for (const [map, mods] of [
      [MAC, { metaKey: true }],
      [WIN, { ctrlKey: true }],
    ] as const) {
      const { d, ran } = setup(map);
      for (const k of [
        key("F5", "F5"),
        key("F7", "F7"),
        key("F12", "F12"),
        key("r", "KeyR", mods),
        key("R", "KeyR", { ...mods, shiftKey: true }),
        key("=", "Equal", mods),
        key("-", "Minus", mods),
        key("0", "Digit0", mods),
        key("f", "KeyF", mods),
        key("g", "KeyG", mods),
        key("s", "KeyS", mods),
        key("J", "KeyJ", { ...mods, shiftKey: true }),
      ]) {
        const outcome = dispatchKey(d, k, BOARD);
        expect(stopsEvent(outcome), `${map.platform} ${logLine(k, outcome)}`).toBe(true);
      }
      expect(ran).toEqual([]);
    }
    const win = setup(WIN).d;
    expect(dispatchKey(win, key("ArrowLeft", "ArrowLeft", { altKey: true }), BOARD).kind).toBe("swallowed");
    // On a Mac ⌥← moves by word.
    const mac = setup(MAC).d;
    expect(dispatchKey(mac, key("ArrowLeft", "ArrowLeft", { altKey: true }), BOARD).kind).toBe("passed");
  });

  it("leaves typing and the Edit keys to the page", () => {
    const { d } = setup(MAC);
    for (const k of [
      key("a", "KeyA"),
      key("c", "KeyC", { metaKey: true }),
      key("v", "KeyV", { metaKey: true }),
      key("z", "KeyZ", { metaKey: true }),
      key("Tab", "Tab"),
      key("ArrowDown", "ArrowDown"),
    ]) {
      expect(dispatchKey(d, k, BOARD).kind).toBe("passed");
    }
  });

  it("lets a text field keep Enter, Backspace and Esc", () => {
    const { d, ran } = setup(MAC, ["open_panel", "close_panel", "send_back", "command_palette"]);
    const field = {
      ...BOARD,
      focusKeeps: keysKeptBy({ tagName: "INPUT", isContentEditable: false, getAttribute: () => null }),
    };
    expect(dispatchKey(d, key("Enter", "Enter"), field).kind).toBe("passed");
    expect(dispatchKey(d, key("Escape", "Escape"), field).kind).toBe("passed");
    expect(dispatchKey(d, key("Backspace", "Backspace", { metaKey: true }), field).kind).toBe("passed");
    expect(dispatchKey(d, key("k", "KeyK", { metaKey: true }), field).kind).toBe("ran");
    expect(dispatchKey(d, key("Enter", "Enter"), BOARD).kind).toBe("ran");
    expect(ran).toEqual(["command_palette", "open_panel"]);
  });

  it("leaves ⌘K / Ctrl+K to the description editor while it has the focus (d0078 D2)", () => {
    const inEditor = (tagName: string, editable: boolean) => ({
      tagName,
      isContentEditable: editable,
      getAttribute: () => null,
      closest: (selector: string) => (selector === ".md-editor" ? {} : null),
    });
    const text = { ...BOARD, focusKeeps: keysKeptBy(inEditor("DIV", true)) };
    const popoverField = { ...BOARD, focusKeeps: keysKeptBy(inEditor("INPUT", false)) };
    const readMode = { ...BOARD, focusKeeps: keysKeptBy(inEditor("A", false)) };
    for (const [map, mods] of [
      [MAC, { metaKey: true }],
      [WIN, { ctrlKey: true }],
    ] as const) {
      const { d, ran } = setup(map, ["command_palette", "switch_project"]);
      const cmdK = key("k", "KeyK", mods);
      expect(dispatchKey(d, cmdK, text)).toEqual({ kind: "passed" });
      expect(dispatchKey(d, cmdK, popoverField)).toEqual({ kind: "passed" });
      // The editor's other keys are the app's as before; read mode and the board keep ⌘K as the palette.
      expect(dispatchKey(d, key("p", "KeyP", mods), text).kind).toBe("ran");
      expect(dispatchKey(d, cmdK, readMode).kind).toBe("ran");
      expect(dispatchKey(d, cmdK, BOARD).kind).toBe("ran");
      expect(ran).toEqual(["switch_project", "command_palette", "command_palette"]);
    }
  });

  it("lets ⌥⌘C through to the description editor's text only, where it's Code block (Q51)", () => {
    const inEditor = (tagName: string, editable: boolean) => ({
      tagName,
      isContentEditable: editable,
      getAttribute: () => null,
      closest: (selector: string) => (selector === ".md-editor" ? {} : null),
    });
    const element = (tagName: string, editable: boolean) => ({
      tagName,
      isContentEditable: editable,
      getAttribute: () => null,
      closest: () => null,
    });
    const at = (focused: Parameters<typeof inEditorText>[0]) => ({
      ...BOARD,
      focusKeeps: keysKeptBy(focused),
      editorText: inEditorText(focused),
    });
    const { d } = setup(MAC);
    const optCmdC = key("ç", "KeyC", { metaKey: true, altKey: true });
    expect(dispatchKey(d, optCmdC, at(inEditor("DIV", true)))).toEqual({ kind: "passed" });
    // The link popover's URL, the Markdown source, read mode, another text field and the board: the inspector key.
    for (const focused of [
      inEditor("INPUT", false),
      inEditor("TEXTAREA", false),
      inEditor("A", false),
      element("DIV", true),
      null,
    ]) {
      expect(dispatchKey(d, optCmdC, at(focused))).toEqual({ kind: "swallowed" });
    }
    // The editor lets only that inspector key through.
    const text = at(inEditor("DIV", true));
    expect(dispatchKey(d, key("i", "KeyI", { metaKey: true, altKey: true }), text).kind).toBe("swallowed");
    expect(dispatchKey(d, key("C", "KeyC", { metaKey: true, shiftKey: true }), text).kind).toBe("swallowed");
    // Ctrl+Alt+C isn't a webview key on Windows and Linux: it reaches the editor anyway.
    const win = setup(WIN).d;
    expect(dispatchKey(win, key("c", "KeyC", { ctrlKey: true, altKey: true }), text)).toEqual({
      kind: "passed",
    });
  });

  it("lets Esc on the description field go to the form, not Close panel (T-0259, d0078 D4)", () => {
    const field = {
      tagName: "DIV",
      isContentEditable: false,
      getAttribute: () => null,
      closest(this: unknown, selector: string) {
        return selector === ".md-editor" ? this : null;
      },
    };
    const { d, ran } = setup(MAC, ["close_panel"]);
    const focused = { ...BOARD, focusKeeps: keysKeptBy(field) };
    expect(dispatchKey(d, key("Escape", "Escape"), focused)).toEqual({ kind: "passed" });
    expect(ran).toEqual([]);
  });

  it("turns ↵ / Esc (Open / close the ticket) off with the letters, leaving them to the page (KEY-02, T-0394)", () => {
    const { d, ran } = setup({ ...MAC, letters: false }, ["open_panel", "close_panel", "send_back"]);
    // NOTE-1's panel is open: Esc on the board runs nothing and isn't stopped, nor is ↵.
    const esc = dispatchKey(d, key("Escape", "Escape"), BOARD);
    expect(esc).toEqual({ kind: "passed" });
    expect(stopsEvent(esc)).toBe(false);
    expect(dispatchKey(d, key("Enter", "Enter"), BOARD)).toEqual({ kind: "passed" });
    // A key with a modifier still runs.
    dispatchKey(d, key("Backspace", "Backspace", { metaKey: true }), BOARD);
    expect(ran).toEqual(["send_back"]);
    const on = setup(MAC, ["close_panel"]);
    expect(dispatchKey(on.d, key("Escape", "Escape"), BOARD).kind).toBe("ran");
    expect(on.ran).toEqual(["close_panel"]);
  });

  it("holds the shortcuts back while a modal is open", () => {
    const { d, ran } = setup(MAC, ["command_palette"]);
    const outcome = dispatchKey(d, key("k", "KeyK", { metaKey: true }), { ...BOARD, modal: true });
    expect(outcome.kind).toBe("unhandled");
    expect(stopsEvent(outcome)).toBe(true);
    expect(ran).toEqual([]);
  });

  it("swallows the webview's keys before the map has arrived", () => {
    const { d } = setup(undefined);
    expect(dispatchKey(d, key("F5", "F5"), BOARD).kind).toBe("swallowed");
    expect(dispatchKey(d, key("k", "KeyK", { ctrlKey: true }), BOARD).kind).toBe("passed");
  });
});

describe("the key dispatcher in a focused terminal", () => {
  it("gives a Mac terminal every key without ⌘, Esc included", () => {
    const { d, ran } = setup(MAC, ["close_panel", "open_panel"]);
    for (const k of [
      key("Escape", "Escape"),
      key("Enter", "Enter"),
      key("c", "KeyC", { ctrlKey: true }),
      key("j", "KeyJ", { ctrlKey: true }),
      key("a", "KeyA"),
      key("F5", "F5"),
    ]) {
      const outcome = dispatchKey(d, k, TERMINAL);
      expect(outcome.kind).toBe("terminal");
      expect(stopsEvent(outcome)).toBe(false);
    }
    expect(ran).toEqual([]);
  });

  it("still runs the ⌘ shortcuts on a Mac, Find among them", () => {
    const { d, ran } = setup(MAC, ["command_palette", "switch_project", "open_activity", "find_in_terminal"]);
    dispatchKey(d, key("k", "KeyK", { metaKey: true }), TERMINAL);
    dispatchKey(d, key("p", "KeyP", { metaKey: true }), TERMINAL);
    dispatchKey(d, key("U", "KeyU", { metaKey: true, shiftKey: true }), TERMINAL);
    expect(dispatchKey(d, key("f", "KeyF", { metaKey: true }), TERMINAL).kind).toBe("ran");
    expect(ran).toEqual(["command_palette", "switch_project", "open_activity", "find_in_terminal"]);
  });

  it("leaves the terminal on ⌃` (Terminals), which a Mac terminal gives the app (d0097)", () => {
    const { d, ran, left } = setup(MAC, ["terminals"]);
    const outcome = dispatchKey(d, key("`", "Backquote", { ctrlKey: true }), TERMINAL);
    expect(outcome).toEqual({ kind: "ran", action: { action: "terminals" } });
    expect(left()).toBe(1);
    expect(ran).toEqual([]);
    // Outside a terminal there's nothing to leave: its screen opens or closes the drawer.
    dispatchKey(d, key("`", "Backquote", { ctrlKey: true }), BOARD);
    expect(left()).toBe(1);
    expect(ran).toEqual(["terminals"]);
  });

  it("gives the app ⌃Tab from a Mac terminal, and keeps every other ⌃ key the terminal's", () => {
    const { d, ran } = setup(MAC, ["next_tab"]);
    expect(dispatchKey(d, key("Tab", "Tab", { ctrlKey: true }), TERMINAL).kind).toBe("ran");
    for (const k of [
      key("c", "KeyC", { ctrlKey: true }),
      key("r", "KeyR", { ctrlKey: true }),
      key("Tab", "Tab"),
    ]) {
      expect(dispatchKey(d, k, TERMINAL).kind, keyName(k)).toBe("terminal");
    }
    expect(ran).toEqual(["next_tab"]);
  });

  it("gives plain Ctrl keys to the session on Windows and Linux and takes the terminal column (d0097)", () => {
    const { d, ran, left } = setup(WIN, ["command_palette", "new_ticket", "find_in_terminal"]);
    for (const k of [
      key("n", "KeyN", { ctrlKey: true }),
      key("r", "KeyR", { ctrlKey: true }),
      key("f", "KeyF", { ctrlKey: true }),
      key("Escape", "Escape"),
    ]) {
      expect(dispatchKey(d, k, TERMINAL).kind).toBe("terminal");
    }
    // Ctrl+K and Ctrl+` are let through; the rest is the Ctrl+Shift column.
    dispatchKey(d, key("k", "KeyK", { ctrlKey: true }), TERMINAL);
    dispatchKey(d, key("N", "KeyN", { ctrlKey: true, shiftKey: true }), TERMINAL);
    dispatchKey(d, key("F", "KeyF", { ctrlKey: true, shiftKey: true }), TERMINAL);
    dispatchKey(d, key("`", "Backquote", { ctrlKey: true }), TERMINAL);
    expect(ran).toEqual(["command_palette", "new_ticket", "find_in_terminal"]);
    expect(left()).toBe(1);
    // Outside the terminal the terminal column means nothing and Ctrl+N is New ticket.
    expect(dispatchKey(d, key("F", "KeyF", { ctrlKey: true, shiftKey: true }), BOARD).kind).not.toBe("ran");
    expect(dispatchKey(d, key("N", "KeyN", { ctrlKey: true, shiftKey: true }), BOARD).kind).not.toBe("ran");
    expect(dispatchKey(d, key("n", "KeyN", { ctrlKey: true }), BOARD).kind).toBe("ran");
  });

  it("leaves unbound Ctrl+Shift keys to the terminal (its copy and paste)", () => {
    const { d } = setup(WIN);
    const copy = dispatchKey(d, key("C", "KeyC", { ctrlKey: true, shiftKey: true }), TERMINAL);
    expect(copy.kind).toBe("passed");
    expect(stopsEvent(copy)).toBe(false);
  });

  it("gives every key to the terminal before the map has arrived", () => {
    const { d } = setup(undefined);
    expect(dispatchKey(d, key("k", "KeyK", { metaKey: true }), TERMINAL).kind).toBe("terminal");
  });
});

describe("keys a focused control keeps", () => {
  const element = (tagName: string, attrs: Record<string, string> = {}, editable = false) => ({
    tagName,
    isContentEditable: editable,
    getAttribute: (name: string) => attrs[name] ?? null,
  });

  it("knows text fields, controls and the rest", () => {
    expect([...keysKeptBy(element("TEXTAREA"))]).toEqual(["Enter", "Backspace", "Escape"]);
    expect([...keysKeptBy(element("DIV", {}, true))]).toEqual(["Enter", "Backspace", "Escape"]);
    expect([...keysKeptBy(element("INPUT", { type: "search" }))]).toEqual(["Enter", "Backspace", "Escape"]);
    expect([...keysKeptBy(element("INPUT", { type: "checkbox" }))]).toEqual(["Enter"]);
    expect([...keysKeptBy(element("BUTTON"))]).toEqual(["Enter"]);
    expect([...keysKeptBy(element("DIV", { role: "option" }))]).toEqual(["Enter"]);
    expect([...keysKeptBy(element("DIV"))]).toEqual([]);
    expect([...keysKeptBy(null)]).toEqual([]);
  });

  it("keeps ↵ and Esc for menus, dialogs and popovers (T-0217)", () => {
    const inside = (tag: string, widget: boolean) => ({
      ...element(tag),
      closest: (selector: string) => (widget && selector.includes('role="dialog"') ? {} : null),
    });
    expect([...keysKeptBy(inside("BUTTON", true))]).toEqual(["Enter", "Escape"]);
    expect([...keysKeptBy(inside("DIV", true))]).toEqual(["Enter", "Escape"]);
    expect([...keysKeptBy(inside("TEXTAREA", true))]).toEqual(["Enter", "Backspace", "Escape"]);
    expect([...keysKeptBy(inside("BUTTON", false))]).toEqual(["Enter"]);
    const { d } = setup(MAC, []);
    const modal = { ...BOARD, modal: true, focusKeeps: keysKeptBy(inside("BUTTON", true)) };
    expect(dispatchKey(d, key("Escape", "Escape"), modal).kind).toBe("passed");
    expect(dispatchKey(d, key("Enter", "Enter"), modal).kind).toBe("passed");
  });
});

describe("the key log", () => {
  it("says where a key went", () => {
    const k = key("k", "KeyK", { metaKey: true });
    expect(logLine(k, { kind: "ran", action: { action: "command_palette" } })).toBe(
      "Meta+K (KeyK) → command_palette",
    );
    expect(logLine(key("F5", "F5"), { kind: "swallowed" })).toBe("F5 (F5) → swallowed (webview key)");
    expect(logLine(k, { kind: "unhandled", action: { action: "project", number: 3 }, swallowed: true })).toBe(
      "Meta+K (KeyK) → project 3 (no handler, swallowed)",
    );
  });
});

// The base map (KEY-01, KEY-02) as `keymap.map` sends it, built from 0097's table the way `keymap::base_bindings`
// places each key (the Rust tests pin the table and the places; these pin the dispatcher on them).
type Slot = [mac: string, win: string, terminal: string];
const letter = (l: string): Slot => [l, l, ""];
const BASE: [KeyAction, Slot[]][] = [
  [
    { action: "command_palette" },
    [
      ["⌘K", "Ctrl+K", "Ctrl+K"],
      ["⇧⌘P", "Ctrl+Shift+P", "Ctrl+Shift+P"],
    ],
  ],
  [{ action: "switch_project" }, [["⌘P", "Ctrl+P", "Ctrl+P"]]],
  [{ action: "project", number: 1 }, [["⌘1", "Ctrl+1", "Ctrl+1"]]],
  [{ action: "next_inbox_item" }, [["⌘J", "Ctrl+J", "Ctrl+Shift+J"]]],
  [{ action: "next_ticket" }, [["⌘]", "Ctrl+]", "Ctrl+Shift+]"], letter("j")]],
  [{ action: "previous_ticket" }, [["⌘[", "Ctrl+[", "Ctrl+Shift+["], letter("k")]],
  [{ action: "filter_board" }, [letter("/")]],
  [{ action: "focus_epic" }, [letter("g e")]],
  [{ action: "show_shortcuts" }, [letter("?")]],
  [{ action: "show_board" }, [["⇧⌘B", "Ctrl+Shift+B", ""], letter("g b")]],
  // 0097's Stop all row, Pause's (d0112): in terminals too.
  [{ action: "pause" }, [["⇧⌘.", "Ctrl+Shift+.", "Ctrl+Shift+."]]],
  [{ action: "open_activity" }, [["⇧⌘U", "Ctrl+Shift+U", "Ctrl+Shift+U"], letter("g a")]],
  [{ action: "settings" }, [["⌘,", "Ctrl+,", "Ctrl+,"]]],
  [{ action: "focus_pane" }, [["⌘L", "Ctrl+L", "Ctrl+Shift+L"]]],
  [{ action: "new_project_agent" }, [["⇧⌘L", "Ctrl+Shift+L", ""]]],
  [{ action: "new_ticket" }, [["⌘N", "Ctrl+N", "Ctrl+Shift+N"], letter("c")]],
  [{ action: "open_panel" }, [["↵", "↵", ""]]],
  [{ action: "close_panel" }, [["Esc", "Esc", ""]]],
  [{ action: "expand_panel" }, [letter("e")]],
  ...(["o", "s", "f", "a"] as const).map((then, i): [KeyAction, Slot[]] => [
    { action: "switch_tab", number: i + 1 },
    [letter(String(i + 1)), letter(`g ${then}`)],
  ]),
  [{ action: "next_tab" }, [["⌃Tab", "Ctrl+Tab", "Ctrl+Tab"]]],
  [{ action: "previous_tab" }, [["⌃⇧Tab", "Ctrl+Shift+Tab", "Ctrl+Shift+Tab"]]],
  [{ action: "approve" }, [["⌘↵", "Ctrl+↵", "Ctrl+Shift+↵"]]],
  [{ action: "send_back" }, [["⌘⌫", "Ctrl+⌫", "Ctrl+Shift+⌫"]]],
  [{ action: "start_dev" }, [["⌘D", "Ctrl+D", "Ctrl+Shift+D"]]],
  [{ action: "terminals" }, [["⌃`", "Ctrl+`", "Ctrl+`"], letter("g t")]],
  [{ action: "find_in_terminal" }, [["⌘F", "", "Ctrl+Shift+F"]]],
  // FIL-11: Files, ⇧⌘O and g f (the sheet's Files tab first); Go to file.
  [{ action: "show_files" }, [["⇧⌘O", "Ctrl+Shift+O", "Ctrl+Shift+O"], letter("g f")]],
  [{ action: "go_to_file" }, [["⌥⌘P", "Ctrl+Alt+P", "Ctrl+Shift+Alt+P"]]],
];

const NAMED: Record<string, string> = { "↵": "Enter", "⌫": "Backspace", Esc: "Escape", Tab: "Tab" };
const CODES: Record<string, string> = {
  "`": "Backquote",
  "[": "BracketLeft",
  "]": "BracketRight",
  ",": "Comma",
  ".": "Period",
};

/** A key text of the map ("⇧⌘L", "Ctrl+Shift+B", "j", "g b") as `keymap::Key` sends it. */
function parseKey(text: string): KeyBinding["key"] {
  if (text.startsWith("g ")) return { kind: "sequence", then: text.slice(2) };
  if (text.length === 1) return { kind: "letter", letter: text };
  const modifiers = { ...NONE };
  let rest = text;
  if (text.includes("+") && text.length > 1) {
    const parts = text.split("+");
    rest = parts.pop() ?? "";
    modifiers.control = parts.includes("Ctrl");
    modifiers.shift = parts.includes("Shift");
    modifiers.alt = parts.includes("Alt");
  } else {
    for (const [glyph, name] of [
      ["⌃", "control"],
      ["⌥", "alt"],
      ["⇧", "shift"],
      ["⌘", "platform"],
    ] as const) {
      if (rest.startsWith(glyph)) {
        modifiers[name] = true;
        rest = rest.slice(1);
      }
    }
    // A Mac text may hold its modifiers in any order ("⇧⌘L").
    for (const [glyph, name] of [
      ["⌃", "control"],
      ["⌥", "alt"],
      ["⇧", "shift"],
      ["⌘", "platform"],
    ] as const) {
      if (rest.startsWith(glyph)) {
        modifiers[name] = true;
        rest = rest.slice(1);
      }
    }
  }
  const named = NAMED[rest];
  const k = named ?? rest.toLowerCase();
  const code = named ?? CODES[rest] ?? (/^[0-9]$/.test(rest) ? `Digit${rest}` : `Key${rest.toUpperCase()}`);
  return { kind: "stroke", stroke: { key: k, code, modifiers } };
}

const MAC_TERMINAL = ["⌃`", "⌃Tab", "⌃⇧Tab"];

function baseMap(platform: "mac" | "win_linux", extra: KeyBinding[] = []): Keymap {
  const bindings: KeyBinding[] = [];
  const add = (action: KeyAction, slot: number, text: string, place: KeyPlace) => {
    bindings.push({ action, slot, text, key: parseKey(text), place, custom: false });
  };
  for (const [action, slots] of BASE) {
    slots.forEach(([mac, win, term], slot) => {
      if (platform === "mac") {
        const key = parseKey(mac);
        const reaches =
          (key.kind === "stroke" && key.stroke.modifiers.platform) || MAC_TERMINAL.includes(mac);
        add(action, slot, mac, reaches ? EVERYWHERE : "outside_terminals");
      } else if (win === term) {
        add(action, slot, win, EVERYWHERE);
      } else {
        if (win !== "") add(action, slot, win, "outside_terminals");
        if (term !== "") add(action, slot, term, INSIDE);
      }
    });
  }
  return {
    platform,
    log: false,
    typing_hint: "Typing? Click where you want to type",
    letters: true,
    actions: [],
    sequences: [],
    terminal_keys: [],
    terminal_app_modifiers: platform === "mac" ? [CMD] : [CTRL_SHIFT, { ...NONE, platform: true }],
    bindings: [...bindings, ...extra],
  };
}

const MAC_0097 = baseMap("mac");
const WIN_0097 = baseMap("win_linux");
/** The example's custom `g` `p` Park (tickets/m29-tickets.md). */
const PARK: KeyBinding = {
  action: { action: "park" },
  slot: 0,
  text: "g p",
  key: { kind: "sequence", then: "p" },
  place: "outside_terminals",
  custom: true,
};

const ALL = BASE.map(([a]) => a.action).concat(["park"]);

/** A key event from a map text: "⇧⌘L", "Ctrl+Shift+B", "j", "?" (typed with Shift, as on a US keyboard). */
function press(text: string): KeyLike {
  const parsed = parseKey(text);
  if (parsed.kind !== "stroke") {
    const typed = parsed.kind === "letter" ? parsed.letter : parsed.then;
    const code = /^[a-z]$/.test(typed)
      ? `Key${typed.toUpperCase()}`
      : /^[0-9]$/.test(typed)
        ? `Digit${typed}`
        : "Slash";
    return key(typed, code, { shiftKey: typed === "?" });
  }
  const { stroke } = parsed;
  const m = stroke.modifiers;
  const typed =
    stroke.key.length === 1 && m.shift && /^[a-z]$/.test(stroke.key) ? stroke.key.toUpperCase() : stroke.key;
  return key(typed, stroke.code, {
    ctrlKey: m.control,
    altKey: m.alt,
    shiftKey: m.shift,
    metaKey: m.platform,
  });
}

/** Presses `texts` one after the other; what ran. */
function run(map: Keymap, texts: string[], ctx: KeyContext = BOARD) {
  const s = setup(map, ALL);
  const outcomes = texts.map((t) => dispatchKey(s.d, press(t), ctx));
  return { ...s, outcomes };
}

describe("0097's keys (KEY-01, KEY-02)", () => {
  it("runs each new and moved key's action on a Mac", () => {
    const cases: [string[], string][] = [
      [["⌘K"], "command_palette"],
      [["⇧⌘P"], "command_palette"],
      [["?"], "show_shortcuts"],
      [["/"], "filter_board"],
      [["⌘L"], "focus_pane"],
      [["⇧⌘L"], "new_project_agent"],
      [["⌃`"], "terminals"],
      [["⌃Tab"], "next_tab"],
      [["⌃⇧Tab"], "previous_tab"],
      [["j"], "next_ticket"],
      [["k"], "previous_ticket"],
      [["e"], "expand_panel"],
      [["c"], "new_ticket"],
      [["g", "b"], "show_board"],
      [["g", "a"], "switch_tab 4"],
      [["g", "t"], "terminals"],
      [["g", "e"], "focus_epic"],
      [["⌘D"], "start_dev"],
      [["↵"], "open_panel"],
      [["Esc"], "close_panel"],
      [["⇧⌘B"], "show_board"],
    ];
    for (const [texts, action] of cases) {
      expect(run(MAC_0097, texts).ran, texts.join(" ")).toEqual([action]);
    }
    for (const n of [1, 2, 3, 4]) {
      expect(run(MAC_0097, [String(n)]).ran).toEqual([`switch_tab ${String(n)}`]);
    }
    expect(run(MAC_0097, ["g", "o"]).ran).toEqual(["switch_tab 1"]);
  });

  it("g a is the open sheet's Activity tab, else Activity (SHB-04)", () => {
    const sheetOpen = setup(MAC_0097, ALL);
    for (const t of ["g", "a"]) dispatchKey(sheetOpen.d, press(t), BOARD);
    expect(sheetOpen.ran).toEqual(["switch_tab 4"]);
    const noSheet = setup(
      MAC_0097,
      ALL.filter((a) => a !== "switch_tab"),
    );
    const outcomes = ["g", "a"].map((t) => dispatchKey(noSheet.d, press(t), BOARD));
    expect(noSheet.ran).toEqual(["open_activity"]);
    expect(outcomes[1]).toEqual({ kind: "ran", action: { action: "open_activity" } });
  });

  it("g f is the open sheet's Files tab, else the Files view (FIL-11)", () => {
    const sheetOpen = setup(MAC_0097, ALL);
    for (const t of ["g", "f"]) dispatchKey(sheetOpen.d, press(t), BOARD);
    expect(sheetOpen.ran).toEqual(["switch_tab 3"]);
    const noSheet = setup(
      MAC_0097,
      ALL.filter((a) => a !== "switch_tab"),
    );
    for (const t of ["g", "f"]) dispatchKey(noSheet.d, press(t), BOARD);
    expect(noSheet.ran).toEqual(["show_files"]);
    const files = setup(MAC_0097, ["show_files"]);
    dispatchKey(files.d, press("⇧⌘O"), BOARD);
    expect(files.ran).toEqual(["show_files"]);
  });

  it("runs each one on Windows and Linux", () => {
    const cases: [string[], string][] = [
      [["Ctrl+K"], "command_palette"],
      [["Ctrl+Shift+P"], "command_palette"],
      [["?"], "show_shortcuts"],
      [["/"], "filter_board"],
      [["Ctrl+L"], "focus_pane"],
      [["Ctrl+Shift+L"], "new_project_agent"],
      [["Ctrl+`"], "terminals"],
      [["Ctrl+Tab"], "next_tab"],
      [["Ctrl+Shift+Tab"], "previous_tab"],
      [["j"], "next_ticket"],
      [["k"], "previous_ticket"],
      [["e"], "expand_panel"],
      [["c"], "new_ticket"],
      [["g", "b"], "show_board"],
      [["g", "t"], "terminals"],
      [["Ctrl+D"], "start_dev"],
      [["Ctrl+Shift+B"], "show_board"],
      [["3"], "switch_tab 3"],
    ];
    for (const [texts, action] of cases) {
      expect(run(WIN_0097, texts).ran, texts.join(" ")).toEqual([action]);
    }
  });

  it("matches letters by character: a Cyrillic layout's j key, ? and / typed with Shift on other layouts", () => {
    const { d, ran } = setup(MAC_0097, ALL);
    dispatchKey(d, key("о", "KeyJ"), BOARD);
    // German: / is ⇧7.
    dispatchKey(d, key("/", "Digit7", { shiftKey: true }), BOARD);
    // J (Shift) isn't j.
    expect(dispatchKey(d, key("J", "KeyJ", { shiftKey: true }), BOARD).kind).not.toBe("ran");
    expect(ran).toEqual(["next_ticket", "filter_board"]);
  });

  it("runs no single letter or sequence while somebody is typing, or in a menu, a dialog or a terminal", () => {
    const at = (letters: LetterHold, more: Partial<KeyContext> = {}): KeyContext => ({
      ...BOARD,
      letters,
      ...more,
    });
    for (const ctx of [at("all"), at("none", { inTerminal: true }), at("none", { modal: true })]) {
      const { ran, outcomes, waiting } = run(MAC_0097, ["j", "c", "/", "?", "e", "1", "g"], ctx);
      expect(ran).toEqual([]);
      expect(waiting()).toBe(false);
      // The letter goes to the focused element (or the terminal) as typed.
      for (const o of outcomes) expect(stopsEvent(o)).toBe(false);
    }
    // A radio group keeps its digits (an answer's 1–4), not the other letters.
    expect(run(MAC_0097, ["1", "j"], at("digits")).ran).toEqual(["next_ticket"]);
  });

  it("knows where somebody types: fields, the editor, the Project agent's box, menus, lists and dialogs", () => {
    const element = (tagName: string, inside: string | null = null, attrs: Record<string, string> = {}) => ({
      tagName,
      isContentEditable: tagName === "EDITABLE",
      getAttribute: (name: string) => attrs[name] ?? null,
      closest: (selector: string) => (inside !== null && selector.split(", ").includes(inside) ? {} : null),
    });
    expect(lettersKeptBy(element("INPUT"))).toBe("all");
    expect(lettersKeptBy(element("INPUT", null, { type: "search" }))).toBe("all");
    // The Project agent's message box is a textarea.
    expect(lettersKeptBy(element("TEXTAREA"))).toBe("all");
    expect(lettersKeptBy(element("EDITABLE"))).toBe("all");
    expect(lettersKeptBy(element("SELECT"))).toBe("all");
    expect(lettersKeptBy(element("A", ".md-editor"))).toBe("all");
    expect(lettersKeptBy(element("BUTTON", '[role="menu"]'))).toBe("all");
    expect(lettersKeptBy(element("DIV", '[role="listbox"]'))).toBe("all");
    expect(lettersKeptBy(element("BUTTON", '[role="dialog"]'))).toBe("all");
    expect(lettersKeptBy(element("DIV", '[role="radiogroup"]'))).toBe("digits");
    // A card, a button on the board and nothing focused.
    expect(lettersKeptBy(element("DIV", null, { role: "button" }))).toBe("none");
    expect(lettersKeptBy(element("BUTTON"))).toBe("none");
    expect(lettersKeptBy(element("INPUT", null, { type: "checkbox" }))).toBe("none");
    expect(lettersKeptBy(null)).toBe("none");
  });

  it("knows a menu or a dialog is open, but not a leaving copy", () => {
    const page = (found: { leaving: boolean }[]) => ({
      querySelectorAll: (selector: string) => {
        expect(selector).toContain('[role="dialog"]');
        return found.map((f) => ({
          closest: (s: string) => (s === "[data-leaving]" && f.leaving ? {} : null),
        }));
      },
    });
    expect(widgetOpen(page([]))).toBe(false);
    expect(widgetOpen(page([{ leaving: true }]))).toBe(false);
    expect(widgetOpen(page([{ leaving: true }, { leaving: false }]))).toBe(true);
  });

  it("runs none with the letters switch off, built-in or custom", () => {
    const off = { ...baseMap("mac", [PARK]), letters: false };
    const { ran, waiting } = run(off, ["j", "c", "/", "?", "1", "g", "b", "g", "p"]);
    expect(ran).toEqual([]);
    expect(waiting()).toBe(false);
    // The modifier keys still work.
    expect(run(off, ["⌘K", "⌘D"]).ran).toEqual(["command_palette", "start_dev"]);
  });

  it("waits after g: the second key runs its sequence; Esc or another key cancels and does nothing more", () => {
    const g = run(MAC_0097, ["g"]);
    expect(g.outcomes).toEqual([{ kind: "waiting" }]);
    expect(stopsEvent({ kind: "waiting" })).toBe(true);
    expect(g.waiting()).toBe(true);
    const board = run(MAC_0097, ["g", "b"]);
    expect(board.ran).toEqual(["show_board"]);
    expect(board.waiting()).toBe(false);
    for (const second of ["Esc", "j", "x", "⌘K"]) {
      const cancelled = run(MAC_0097, ["g", second]);
      expect(cancelled.ran, second).toEqual([]);
      expect(cancelled.outcomes[1], second).toEqual({ kind: "cancelled" });
      expect(stopsEvent({ kind: "cancelled" })).toBe(true);
      expect(cancelled.waiting()).toBe(false);
    }
    // Once cancelled, the next key is itself again.
    expect(run(MAC_0097, ["g", "Esc", "j"]).ran).toEqual(["next_ticket"]);
    // Plain b isn't Board.
    expect(run(MAC_0097, ["b"]).ran).toEqual([]);
  });

  it("runs a custom g p as Park", () => {
    expect(run(baseMap("mac", [PARK]), ["g", "p"]).ran).toEqual(["park"]);
    expect(run(baseMap("win_linux", [PARK]), ["g", "p"]).ran).toEqual(["park"]);
    expect(run(MAC_0097, ["g", "p"]).ran).toEqual([]);
  });

  it("gives a key typed into a field after g has moved the focus to the field", () => {
    const { d, ran } = setup(MAC_0097, ALL);
    dispatchKey(d, press("g"), BOARD);
    expect(dispatchKey(d, press("b"), { ...BOARD, letters: "all" })).toEqual({ kind: "passed" });
    expect(ran).toEqual([]);
  });
});

describe("0097's terminal rule (KEY-03)", () => {
  it("on Windows and Linux in a terminal: the let-through keys, the Shift forms, and only the terminal column", () => {
    const t = (texts: string[]) => run(WIN_0097, texts, TERMINAL);
    expect(t(["Ctrl+K"]).ran).toEqual(["command_palette"]);
    expect(t(["Ctrl+`"]).left()).toBe(1);
    expect(t(["Ctrl+Shift+D"]).ran).toEqual(["start_dev"]);
    expect(t(["Ctrl+D"]).outcomes).toEqual([{ kind: "terminal" }]);
    // Ctrl+Shift+B is nothing there (never Board; the pane's hide / show key went, d0113); Ctrl+Shift+L opens or
    // closes the Project agent, never New conversation.
    expect(t(["Ctrl+Shift+B"]).ran).toEqual([]);
    expect(t(["Ctrl+Shift+L"]).ran).toEqual(["focus_pane"]);
    expect(t(["Ctrl+Tab"]).ran).toEqual(["next_tab"]);
    expect(t(["Ctrl+1"]).ran).toEqual(["project 1"]);
    // Letters and sequences are the terminal's.
    expect(t(["j", "g", "b"]).outcomes).toEqual([
      { kind: "terminal" },
      { kind: "terminal" },
      { kind: "terminal" },
    ]);
    // Outside it, Ctrl+Shift+B is Board and Ctrl+Shift+L New Project agent.
    expect(run(WIN_0097, ["Ctrl+Shift+B", "Ctrl+Shift+L"]).ran).toEqual(["show_board", "new_project_agent"]);
  });

  it("on a Mac in a terminal: ⌃` leaves it, ⌃Tab switches the tab, ⌃C and letters reach the terminal", () => {
    const t = (texts: string[]) => run(MAC_0097, texts, TERMINAL);
    const leave = t(["⌃`"]);
    expect(leave.left()).toBe(1);
    expect(leave.ran).toEqual([]);
    expect(t(["⌃Tab", "⌃⇧Tab"]).ran).toEqual(["next_tab", "previous_tab"]);
    expect(t(["⌃C", "⌃D", "j", "g"]).outcomes.map((o) => o.kind)).toEqual([
      "terminal",
      "terminal",
      "terminal",
      "terminal",
    ]);
    expect(t(["⌘K", "⌘D", "⌘L"]).ran).toEqual(["command_palette", "start_dev", "focus_pane"]);
  });
});

describe("over a modal (`modal.rs`)", () => {
  const MODAL = { ...BOARD, modal: true, letters: "all" as const };
  it("still leaves a focused terminal on ⌃` and finds in it, and runs nothing else", () => {
    const inTerminal = run(MAC_0097, ["⌃`", "⌘F", "⌘K", "⌘D"], { ...MODAL, inTerminal: true });
    expect(inTerminal.left()).toBe(1);
    expect(inTerminal.ran).toEqual(["find_in_terminal"]);
    // Outside a terminal ⌃` doesn't open the drawer behind the dialog.
    const outside = run(MAC_0097, ["⌃`", "⌘K"], MODAL);
    expect(outside.ran).toEqual([]);
    expect(outside.outcomes.every(stopsEvent)).toBe(true);
    const win = run(WIN_0097, ["Ctrl+`", "Ctrl+Shift+F"], { ...MODAL, inTerminal: true });
    expect(win.left()).toBe(1);
    expect(win.ran).toEqual(["find_in_terminal"]);
  });
});

describe("the description editor's ⌘B and ⌘K (0097, d0078)", () => {
  const inEditor = {
    tagName: "DIV",
    isContentEditable: true,
    getAttribute: () => null,
    closest: (selector: string) => (selector === ".md-editor" ? {} : null),
  };
  const text: KeyContext = { ...BOARD, focusKeeps: keysKeptBy(inEditor), letters: lettersKeptBy(inEditor) };

  it("leaves ⌘B / Ctrl+B to its Bold and ⌘K / Ctrl+K to its Link; ⇧⌘B stays Board (Q221)", () => {
    for (const [map, keys, board] of [
      [MAC_0097, ["⌘B", "⌘K"], "⇧⌘B"],
      [WIN_0097, ["Ctrl+B", "Ctrl+K"], "Ctrl+Shift+B"],
    ] as const) {
      const { outcomes, ran } = run(map, [...keys], text);
      expect(outcomes).toEqual([{ kind: "passed" }, { kind: "passed" }]);
      expect(ran).toEqual([]);
      expect(run(map, [board], text).ran).toEqual(["show_board"]);
      // Elsewhere ⌘B is no app key (d0113).
      expect(run(map, [keys[0]]).ran).toEqual([]);
    }
  });
});

describe("the key log of the g wait", () => {
  it("says the key waited or cancelled", () => {
    expect(logLine(key("g", "KeyG"), { kind: "waiting" })).toBe("G (KeyG) → waiting for the second key");
    expect(logLine(key("x", "KeyX"), { kind: "cancelled" })).toBe("X (KeyX) → cancelled the g wait");
  });
});

describe("a recording shortcut cell (T-0395)", () => {
  it("gets every key: nothing runs, nothing is swallowed, the g wait ends", () => {
    const recorder = {
      tagName: "BUTTON",
      isContentEditable: false,
      getAttribute: () => null,
      closest: (selector: string) => (selector === KEY_RECORDER ? {} : null),
    };
    expect(inKeyRecorder(recorder)).toBe(true);
    expect(inKeyRecorder(null)).toBe(false);
    const ctx = { ...BOARD, modal: true, recording: inKeyRecorder(recorder) };
    const { d, ran, waiting } = setup(MAC_0097, ALL);
    d.wait(true);
    for (const text of ["⌘J", "⌘R", "Esc", "g", "j", "⌃`"]) {
      expect(dispatchKey(d, press(text), ctx), text).toEqual({ kind: "passed" });
    }
    expect(ran).toEqual([]);
    expect(waiting()).toBe(false);
  });
});

describe("a fast burst of letters with nothing focused (d0126)", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** A dispatcher with the guard on a fake clock; `dropped` records each dropped key (`true`: the burst began). */
  function guarded(map: Keymap = MAC_0097) {
    vi.useFakeTimers();
    const s = setup(map, ALL);
    const dropped: boolean[] = [];
    let notes = 0;
    s.d.burst = createBurst({
      now: () => Date.now(),
      note: () => notes++,
      dropped: (first) => dropped.push(first),
    });
    /** Types `text` one key every `gap` ms; what became of each key. */
    const type = (text: string, gap: number, ctx: KeyContext = BOARD) =>
      Array.from(text).map((ch) => {
        const outcome = dispatchKey(s.d, press(ch), ctx);
        vi.advanceTimersByTime(gap);
        return outcome;
      });
    return { ...s, dropped, notes: () => notes, type };
  }

  it("ten letters in a second run nothing after the first, which the guard closes again", () => {
    const g = guarded();
    // `c` (New ticket) first, then nine more: 100 ms apart.
    const outcomes = g.type("cjkjkejkje", 100);
    expect(g.ran).toEqual(["new_ticket"]);
    expect(outcomes.slice(1).every((o) => o.kind === "burst" && stopsEvent(o))).toBe(true);
    // The burst began on the second letter: what `c` opened closes, the hint shows; the rest are only dropped.
    expect(g.dropped).toEqual([true, false, false, false, false, false, false, false, false]);
    expect(g.notes()).toBe(1);
  });

  it("a burst that starts with letters that run nothing runs nothing either", () => {
    const g = guarded();
    g.type("hello world", 90);
    expect(g.ran).toEqual([]);
    expect(g.waiting()).toBe(false);
  });

  it("letters typed one at a time still run, and so does the first after a pause", () => {
    const g = guarded();
    g.type("jjj", 1000);
    expect(g.ran).toEqual(["next_ticket", "next_ticket", "next_ticket"]);
    g.type("xx", 50);
    vi.advanceTimersByTime(BURST_MS);
    g.type("k", 0);
    expect(g.ran).toEqual(["next_ticket", "next_ticket", "next_ticket", "previous_ticket"]);
  });

  it("g b works however fast it is typed; typing on after it is a burst", () => {
    const g = guarded();
    const [first, second] = g.type("gb", 50);
    expect(first?.kind).toBe("waiting");
    expect(second).toEqual({ kind: "ran", action: { action: "show_board" } });
    vi.advanceTimersByTime(1000);
    const outcomes = g.type("gbxy", 50);
    expect(outcomes.map((o) => o.kind)).toEqual(["waiting", "ran", "burst", "burst"]);
    expect(g.dropped).toEqual([true, false]);
  });

  it("holding j is no burst: the key and its ten repeats in a second move eleven times", () => {
    const g = guarded();
    const held = press("j");
    const outcomes = [held, ...Array.from({ length: 10 }, () => ({ ...held, repeat: true }))].map((e) => {
      const outcome = dispatchKey(g.d, e, BOARD);
      vi.advanceTimersByTime(100);
      return outcome;
    });
    expect(outcomes.every((o) => o.kind === "ran")).toBe(true);
    expect(g.ran).toEqual(Array.from({ length: 11 }, () => "next_ticket"));
    expect(g.dropped).toEqual([]);
  });

  it("c then fast different letters is still a burst, and a repeat inside one leaves it on", () => {
    const g = guarded();
    const outcomes = g.type("cxy", 50);
    expect(outcomes.map((o) => o.kind)).toEqual(["ran", "burst", "burst"]);
    dispatchKey(g.d, { ...press("y"), repeat: true }, BOARD);
    vi.advanceTimersByTime(50);
    expect(dispatchKey(g.d, press("z"), BOARD).kind).toBe("burst");
    expect(g.ran).toEqual(["new_ticket"]);
    expect(g.dropped).toEqual([true, false, false]);
  });

  it("a g inside a burst starts no wait", () => {
    const g = guarded();
    const outcomes = g.type("gzgz", 100);
    // `g z` is no sequence: the wait ends on z, and the second g is part of the burst, so no hint starts.
    expect(outcomes.map((o) => o.kind)).toEqual(["waiting", "cancelled", "burst", "burst"]);
    expect(g.waiting()).toBe(false);
  });

  it("↵ and ⌫ in a burst are dropped too; any other key ends it", () => {
    const g = guarded();
    const outcomes = g.type("ab", 50);
    expect(outcomes.map((o) => o.kind)).toEqual(["passed", "burst"]);
    expect(dispatchKey(g.d, press("↵"), BOARD).kind).toBe("burst");
    vi.advanceTimersByTime(50);
    // ⌘K is no typing: the palette opens and the guard starts over.
    expect(dispatchKey(g.d, press("⌘K"), BOARD).kind).toBe("ran");
    expect(dispatchKey(g.d, press("j"), BOARD).kind).toBe("ran");
  });

  it("/ puts the keyboard in the filter on purpose: what follows is typed there", () => {
    const g = guarded();
    expect(dispatchKey(g.d, press("/"), BOARD).kind).toBe("ran");
    vi.advanceTimersByTime(80);
    const typing = {
      ...BOARD,
      letters: "all" as const,
      focusKeeps: new Set(["Enter", "Backspace", "Escape"]),
    };
    expect(g.type("fix", 80, typing).map((o) => o.kind)).toEqual(["passed", "passed", "passed"]);
    expect(g.dropped).toEqual([]);
  });

  it("leaves typing in a field, a terminal and the letters switch alone", () => {
    const g = guarded();
    const field = { ...BOARD, letters: "all" as const };
    expect(g.type("hello", 30, field).every((o) => o.kind === "passed")).toBe(true);
    expect(g.type("hello", 30, TERMINAL).every((o) => o.kind === "terminal")).toBe(true);
    const off = guarded({ ...MAC_0097, letters: false });
    expect(off.type("cjk", 30).every((o) => o.kind === "passed")).toBe(true);
    expect(g.dropped).toEqual([]);
    expect(off.dropped).toEqual([]);
  });
});
