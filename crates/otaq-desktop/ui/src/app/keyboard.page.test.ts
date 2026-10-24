// @vitest-environment happy-dom
// The dispatcher in a page (T-0391, KEY-02): the `g` hint shows after `g` with the user's sequences and goes on the
// letter, after the wait and on Esc, never while typing or with letters off; the description editor keeps ⌘B (Bold)
// and ⌘K (Link) from the app's Hide / show pane and palette.
import type { Editor } from "@tiptap/core";
import { act, createElement, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import type { KeyBinding } from "../generated/KeyBinding";
import type { Keymap } from "../generated/Keymap";
import type { KeyModifiers } from "../generated/KeyModifiers";
import { MarkdownSurface, useMarkdownEditor } from "../markdown/Editor";
import { linkUi } from "../markdown/link/links";
import { installKeys, keymap, SEQUENCE_WAIT_MS, sequenceWait, TYPING_HINT_MS, typingHint } from "./keyboard";
import { overlays } from "./overlays";
import { createRegistries, type Registries } from "./registry";
import { SequenceHint } from "./SequenceHint";
import { TypingHint } from "./TypingHint";

vi.mock("../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  // The event stream never opens here: the test sets the map itself.
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const NONE: KeyModifiers = { control: false, alt: false, shift: false, platform: false, function: false };
const mac = /Mac|iP(hone|[oa]d)/.test(navigator.platform);
// ProseMirror's Mod and the map's: ⌘ where the platform says Mac, else Ctrl.
const MOD = mac ? { ...NONE, platform: true } : { ...NONE, control: true };

const binding = (action: KeyBinding["action"], key: KeyBinding["key"], text: string): KeyBinding => ({
  action,
  slot: 0,
  text,
  key,
  place: "outside_terminals",
  custom: false,
});
const sequence = (action: KeyBinding["action"], then: string) =>
  binding(action, { kind: "sequence", then }, `g ${then}`);

function map(letters: boolean): Keymap {
  return {
    platform: mac ? "mac" : "win_linux",
    letters,
    log: false,
    typing_hint: "Typing? Click where you want to type",
    actions: [],
    terminal_keys: [],
    terminal_app_modifiers: [],
    bindings: [
      sequence({ action: "show_board" }, "b"),
      sequence({ action: "terminals" }, "t"),
      sequence({ action: "park" }, "p"),
      binding({ action: "next_ticket" }, { kind: "letter", letter: "j" }, "j"),
      binding({ action: "new_ticket" }, { kind: "letter", letter: "c" }, "c"),
      binding(
        { action: "open_activity" },
        { kind: "stroke", stroke: { key: "b", code: "KeyB", modifiers: MOD } },
        "⌘B",
      ),
      binding(
        { action: "command_palette" },
        { kind: "stroke", stroke: { key: "k", code: "KeyK", modifiers: MOD } },
        "⌘K",
      ),
    ],
    sequences: [
      { then: "b", name: "Board" },
      { then: "t", name: "Terminals" },
      { then: "p", name: "Park" },
    ],
  };
}

let root: Root | null = null;
let host: HTMLElement | null = null;
let stop: (() => void) | null = null;
let app: Registries;
let ran: string[];

beforeEach(() => {
  app = createRegistries();
  ran = [];
  for (const name of [
    "show_board",
    "terminals",
    "park",
    "next_ticket",
    "open_activity",
    "command_palette",
  ] as const)
    app.shortcuts.on(name, () => {
      ran.push(name);
    });
  // New ticket as the editor opens it: the overlay, its title field focused.
  app.shortcuts.on("new_ticket", () => {
    ran.push("new_ticket");
    overlays.open("new_ticket");
    const title = document.createElement("input");
    title.className = "title";
    host?.append(title);
    title.focus();
  });
  stop = installKeys(app);
  keymap.set(map(true));
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  overlays.store.set([]);
  act(() => root?.unmount());
  host?.remove();
  stop?.();
  vi.useRealTimers();
});

function press(k: string, mods: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key: k,
    code: /^[a-z]$/.test(k) ? `Key${k.toUpperCase()}` : k,
    bubbles: true,
    cancelable: true,
    ...mods,
  });
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(event);
  });
  return event;
}

const hint = () => host?.querySelector(".g-hint-layer:not([data-leaving])")?.textContent ?? null;

test("the hint shows after g with the user's sequences, and goes on the letter", () => {
  act(() => root?.render(createElement(SequenceHint)));
  expect(hint()).toBeNull();
  expect(press("g").defaultPrevented).toBe(true);
  expect(hint()).toBe("g→bBoard·tTerminals·pPark");
  press("p");
  expect(ran).toEqual(["park"]);
  expect(hint()).toBeNull();
});

test("the hint goes after the wait, and the letter then does nothing", () => {
  vi.useFakeTimers();
  act(() => root?.render(createElement(SequenceHint)));
  press("g");
  expect(sequenceWait.get()).toBe(true);
  act(() => {
    vi.advanceTimersByTime(SEQUENCE_WAIT_MS);
  });
  expect(hint()).toBeNull();
  press("b");
  expect(ran).toEqual([]);
});

test("Esc ends the wait and the hint, and does nothing more", () => {
  act(() => root?.render(createElement(SequenceHint)));
  press("g");
  expect(press("Escape").defaultPrevented).toBe(true);
  expect(hint()).toBeNull();
  press("b");
  expect(ran).toEqual([]);
});

test("no hint while typing: g and j type into the field", () => {
  act(() => root?.render(createElement("div", null, createElement(SequenceHint), createElement("input"))));
  const field = host?.querySelector("input");
  act(() => field?.focus());
  expect(press("g").defaultPrevented).toBe(false);
  expect(press("j").defaultPrevented).toBe(false);
  expect(hint()).toBeNull();
  expect(ran).toEqual([]);
});

test("no hint, and no letter, with letters off", () => {
  keymap.set(map(false));
  act(() => root?.render(createElement(SequenceHint)));
  expect(press("g").defaultPrevented).toBe(false);
  press("j");
  expect(hint()).toBeNull();
  expect(ran).toEqual([]);
});

test("in the description editor ⌘B makes text bold and ⌘K opens the link; neither runs the app's action", () => {
  const shown: { editor?: Editor } = {};
  function Description() {
    const editor = useMarkdownEditor({ value: "hello", label: "What and why" });
    useEffect(() => {
      shown.editor = editor;
    });
    return createElement(MarkdownSurface, { editor });
  }
  act(() => root?.render(createElement(Description)));
  const editor = shown.editor;
  if (!editor) throw new Error("not rendered");
  act(() => {
    editor.view.focus();
    editor.commands.selectAll();
  });
  const mods = mac ? { metaKey: true } : { ctrlKey: true };
  press("b", { ...mods, keyCode: 66 });
  expect(editor.isActive("bold")).toBe(true);
  press("k", { ...mods, keyCode: 75 });
  expect(linkUi(editor).get().popover).not.toBeNull();
  expect(ran).toEqual([]);
  // Outside the editor a ⌘B the user recorded (Activity here) is the app's again.
  act(() => {
    (document.activeElement as HTMLElement | null)?.blur();
  });
  press("b", mods);
  expect(ran).toEqual(["open_activity"]);
});

test("a fast burst after c never leaves New ticket open, types nothing and shows the typing hint (d0126)", () => {
  vi.useFakeTimers();
  act(() => root?.render(createElement(TypingHint)));
  const typed = Array.from("create a habit tracker").map((k) => {
    const event = press(k);
    act(() => {
      vi.advanceTimersByTime(60);
    });
    return event;
  });
  expect(ran).toEqual(["new_ticket"]);
  expect(overlays.store.get()).toEqual([]);
  expect(typed.slice(1).every((e) => e.defaultPrevented)).toBe(true);
  // The title field lost the keyboard and got none of it.
  const title = host?.querySelector<HTMLInputElement>("input.title");
  expect(title?.value).toBe("");
  expect(document.activeElement).not.toBe(title);
  expect(host?.textContent).toContain("Typing? Click where you want to type");
  act(() => {
    vi.advanceTimersByTime(TYPING_HINT_MS);
  });
  expect(typingHint.get()).toBe(false);
  // After the pause a single letter runs again.
  press("j");
  expect(ran).toEqual(["new_ticket", "next_ticket"]);
});

test("c on its own opens New ticket, and its title takes what is typed after a moment", () => {
  vi.useFakeTimers();
  press("c");
  expect(overlays.store.get().map((o) => o.id)).toEqual(["new_ticket"]);
  act(() => {
    vi.advanceTimersByTime(600);
  });
  expect(press("F").defaultPrevented).toBe(false);
  expect(press("i").defaultPrevented).toBe(false);
  expect(overlays.store.get().map((o) => o.id)).toEqual(["new_ticket"]);
});

test("g b typed fast still goes to the board", () => {
  vi.useFakeTimers();
  press("g");
  act(() => {
    vi.advanceTimersByTime(40);
  });
  press("b");
  expect(ran).toEqual(["show_board"]);
});
