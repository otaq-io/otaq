// @vitest-environment happy-dom
// The comment box writes Markdown like GitHub's (tuq 111, plan comments-as-markdown piece 7), with the page's key
// dispatcher installed as in the app: Write and Preview tabs (Preview draws the comment as the thread will; Write
// brings the text back with its selection), the toolbar's buttons, the shortcuts ⌘B ⌘I ⌘E ⌘K on a Mac and Ctrl+B …
// on Windows and Linux, which belong to the box while its text has the focus while ⌘K stays the command palette
// everywhere else, ⌘Z after a button, and ⌘↵ and Ctrl+V (decision 048) left as they were.
import { act, createElement, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { installKeys, keymap } from "../../app/keyboard";
import { overlays } from "../../app/overlays";
import { createRegistries, type Registries } from "../../app/registry";
import type { CommentBoxView } from "../../generated/CommentBoxView";
import type { CommentFormat } from "../../generated/CommentFormat";
import type { KeyBinding } from "../../generated/KeyBinding";
import type { Keymap } from "../../generated/Keymap";
import type { KeyModifiers } from "../../generated/KeyModifiers";
import { CommentBox } from "./CommentBox";
import type { BoxValue } from "./commentRules";
import { WRITING } from "./editor_fixtures";

vi.mock(import("../../tauri"), async (original) => ({
  ...(await original()),
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise<void>(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const NONE: KeyModifiers = { control: false, alt: false, shift: false, platform: false, function: false };

/** The base map's ⌘K (Ctrl+K), and a ⌘B (Ctrl+B) the user recorded for Activity, on one platform. */
function mapFor(mac: boolean): Keymap {
  const mod = mac ? { ...NONE, platform: true } : { ...NONE, control: true };
  const stroke = (action: KeyBinding["action"], key: string, text: string): KeyBinding => ({
    action,
    slot: 0,
    text,
    key: { kind: "stroke", stroke: { key, code: `Key${key.toUpperCase()}`, modifiers: mod } },
    place: "outside_terminals",
    custom: false,
  });
  return {
    platform: mac ? "mac" : "win_linux",
    letters: true,
    log: false,
    typing_hint: "Typing? Click where you want to type",
    actions: [],
    sequences: [],
    terminal_keys: [],
    terminal_app_modifiers: [],
    bindings: [
      stroke({ action: "command_palette" }, "k", mac ? "⌘K" : "Ctrl+K"),
      stroke({ action: "open_activity" }, "b", mac ? "⌘B" : "Ctrl+B"),
    ],
  };
}

/** The view Rust gives on each platform: the tooltips print ⌘ on a Mac, Ctrl on Windows and Linux. */
function viewFor(mac: boolean): CommentBoxView {
  return {
    label: null,
    placeholder: "Add a comment for the next stage…",
    submit: "Comment",
    footer: null,
    footer_icon: "arrow-right",
    pasted: "Pasted text",
    remove_file: "Remove the file",
    open: "Open",
    remove: "Remove the pasted text",
    attach: "Attach",
    attach_title: "Attach files to the comment",
    optional: false,
    limits: {
      report_bytes: 20 * 1024 * 1024,
      report_label: "20 MB",
      message_bytes: 16 * 1024 * 1024 - 64 * 1024,
      too_long_to_send: "This comment is too long to send.",
    },
    tell_agent: "Tell Strong now",
    writing: {
      ...WRITING,
      buttons: WRITING.buttons.map((b) => ({
        ...b,
        keys: b.keys === null || mac ? b.keys : b.keys.replace("⌘", "Ctrl+"),
      })),
    },
  };
}

const PLATFORMS = [
  { name: "macOS", mac: true, mods: { metaKey: true } },
  { name: "Windows and Linux", mac: false, mods: { ctrlKey: true } },
] as const;

let root: Root | null = null;
let host: HTMLElement;
let stop: (() => void) | null = null;
let app: Registries;
let ran: string[];
let sent: { how: string; value: BoxValue }[];

beforeEach(() => {
  app = createRegistries();
  ran = [];
  sent = [];
  for (const name of ["command_palette", "open_activity"] as const)
    app.shortcuts.on(name, () => {
      ran.push(name);
    });
  stop = installKeys(app);
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  overlays.store.set([]);
  act(() => root?.unmount());
  root = null;
  host.remove();
  stop?.();
  document.body.innerHTML = "";
  Reflect.deleteProperty(document, "execCommand");
});

/** The box with its own state, as the thread holds it; `clears`: a sent comment empties it, as the thread's does. */
function Box({ view, text, clears = false }: { view: CommentBoxView; text: string; clears?: boolean }) {
  const [value, setValue] = useState<BoxValue>({ text, attachments: [] });
  return createElement(CommentBox, {
    ticket: "SHOP-34",
    view,
    value,
    onChange: setValue,
    onSubmit: () => {
      sent.push({ how: "comment", value });
      if (clears) setValue({ text: "", attachments: [] });
    },
    onTell: () => sent.push({ how: "tell", value }),
  });
}

/** A fresh box holding `text`. */
function mount(mac: boolean, text = "", clears = false): HTMLTextAreaElement {
  act(() => {
    keymap.set(mapFor(mac));
    root?.unmount();
  });
  root = createRoot(host);
  act(() => root?.render(createElement(Box, { view: viewFor(mac), text, clears })));
  return area();
}

/** The browser's `insertText` and `delete` as WebKit and WebView2 run them on the focused textarea (the test runner's
 * DOM has neither); returns the commands run, so a test sees an edit went through the textarea's own editing. */
function browserEditing(): string[] {
  const commands: string[] = [];
  const editing = (name: string, _ui: boolean, value = "") => {
    const el = document.activeElement;
    if (!(el instanceof HTMLTextAreaElement) || (name !== "insertText" && name !== "delete")) return false;
    commands.push(name);
    const { selectionStart: start, selectionEnd: end } = el;
    const insert = name === "insertText" ? value : "";
    Reflect.set(
      HTMLTextAreaElement.prototype,
      "value",
      el.value.slice(0, start) + insert + el.value.slice(end),
      el,
    );
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.setSelectionRange(start + insert.length, start + insert.length);
    return true;
  };
  Object.defineProperty(document, "execCommand", { configurable: true, value: editing });
  return commands;
}

/** A paste of `text` (and `files`) into the box, as the webview sends it. */
function pasteInto(el: HTMLTextAreaElement, text: string, files: File[] = []): Event {
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", { value: { getData: () => text, files } });
  act(() => {
    el.dispatchEvent(event);
  });
  return event;
}

function area(): HTMLTextAreaElement {
  const found = host.querySelector("textarea");
  if (!found) throw new Error("no box");
  return found;
}

/** Puts the focus in the box and selects `start`…`end`. */
function select(el: HTMLTextAreaElement, start: number, end = start) {
  act(() => {
    el.focus();
    el.setSelectionRange(start, end);
  });
}

/** A key pressed where the focus is, as the webview sends it (the dispatcher listens on the window first). */
function press(key: string, mods: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key,
    code: /^[a-z]$/i.test(key) ? `Key${key.toUpperCase()}` : key,
    bubbles: true,
    cancelable: true,
    ...mods,
  });
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(event);
  });
  return event;
}

function selected(el: HTMLTextAreaElement): string {
  return el.value.slice(el.selectionStart, el.selectionEnd);
}

function tab(name: string): HTMLButtonElement {
  const found = [...host.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find(
    (t) => t.textContent === name,
  );
  if (!found) throw new Error(`no ${name} tab`);
  return found;
}

function tool(label: string): HTMLButtonElement {
  const found = host.querySelector<HTMLButtonElement>(`.comment-box-tools button[aria-label="${label}"]`);
  if (!found) throw new Error(`no ${label} button`);
  return found;
}

describe("Write and Preview", () => {
  it("Preview draws the text as the thread will, and Write brings the text back with its selection", () => {
    const el = mount(true, "Ship **this** and `that`");
    select(el, 7, 11);
    expect(tab("Write").getAttribute("aria-selected")).toBe("true");
    act(() => {
      tab("Preview").click();
    });
    expect(tab("Preview").getAttribute("aria-selected")).toBe("true");
    // The textarea only hides; the toolbar goes, and the comment is drawn by the thread's renderer.
    expect(area().hidden).toBe(true);
    expect(host.querySelector(".comment-box-tools")).toBeNull();
    const drawn = host.querySelector(".comment-box-preview .md-comment .ProseMirror");
    expect(drawn?.querySelector("strong")?.textContent).toBe("this");
    expect(drawn?.querySelector("code")?.textContent).toBe("that");
    // Somewhere else had the focus meanwhile.
    act(() => {
      (document.activeElement as HTMLElement | null)?.blur();
    });
    act(() => {
      tab("Write").click();
    });
    expect(area().hidden).toBe(false);
    expect(host.querySelector(".comment-box-preview")).toBeNull();
    expect(document.activeElement).toBe(area());
    expect([area().selectionStart, area().selectionEnd]).toEqual([7, 11]);
    expect(selected(area())).toBe("this");
  });

  it("says there is nothing to preview for an empty box", () => {
    mount(true, "  ");
    act(() => {
      tab("Preview").click();
    });
    expect(host.querySelector(".comment-box-preview")?.textContent).toBe("Nothing to preview");
  });

  it("comes back to Write after a comment is sent from Preview, as GitHub's box does", () => {
    mount(true, "**done**", true);
    act(() => {
      tab("Preview").click();
    });
    act(() => {
      [...host.querySelectorAll("button")].find((b) => b.textContent === "Comment")?.click();
    });
    expect(sent.map((s) => s.value.text)).toEqual(["**done**"]);
    expect(tab("Write").getAttribute("aria-selected")).toBe("true");
    expect(area().hidden).toBe(false);
    expect(area().value).toBe("");
    expect(document.activeElement).toBe(area());
    expect(host.querySelector(".comment-box-preview")).toBeNull();
    expect(host.querySelector(".comment-box-tools")).not.toBeNull();
  });
});

describe("lists continue on Enter", () => {
  it("Enter in an item starts the next one through the textarea's own editing, and on an empty item ends the list", () => {
    const commands = browserEditing();
    const el = mount(true, "Steps:\n1. build");
    select(el, el.value.length);
    expect(press("Enter").defaultPrevented).toBe(true);
    expect(area().value).toBe("Steps:\n1. build\n2. ");
    expect(area().selectionStart).toBe(area().value.length);
    // An empty item: its marker goes and the list ends.
    expect(press("Enter").defaultPrevented).toBe(true);
    expect(area().value).toBe("Steps:\n1. build\n");
    expect(commands).toEqual(["insertText", "delete"]);
    // The next Enter, outside the list, is the textarea's own.
    expect(press("Enter").defaultPrevented).toBe(false);
    expect(area().value).toBe("Steps:\n1. build\n");
  });

  it("continues a task list with an unticked box", () => {
    browserEditing();
    const el = mount(true, "- [x] tests pass");
    select(el, el.value.length);
    press("Enter");
    expect(area().value).toBe("- [x] tests pass\n- [ ] ");
  });

  it("leaves Enter outside a list, and Enter with a selection, to the textarea", () => {
    const el = mount(true, "plain line\n- item");
    select(el, 10);
    expect(press("Enter").defaultPrevented).toBe(false);
    select(el, 13, 17);
    expect(press("Enter").defaultPrevented).toBe(false);
    expect(area().value).toBe("plain line\n- item");
  });

  it("leaves Enter that ends an IME composition to the composition", () => {
    const el = mount(true, "- 日本");
    select(el, el.value.length);
    expect(press("Enter", { isComposing: true }).defaultPrevented).toBe(false);
    expect(area().value).toBe("- 日本");
  });

  it.each(PLATFORMS)("keeps Shift+Enter a line break and ⌘↵ a send in a list on $name", ({ mac, mods }) => {
    const el = mount(mac, "- one");
    select(el, 5);
    expect(press("Enter", { shiftKey: true }).defaultPrevented).toBe(false);
    expect(area().value).toBe("- one");
    expect(sent).toEqual([]);
    expect(press("Enter", mods).defaultPrevented).toBe(true);
    expect(area().value).toBe("- one");
    expect(sent.map((s) => [s.how, s.value.text])).toEqual([["comment", "- one"]]);
  });
});

describe("a URL pasted over selected text", () => {
  it("makes a link of the selection, through the textarea's own editing", () => {
    const commands = browserEditing();
    const el = mount(true, "see the docs for it");
    select(el, 8, 12);
    const paste = pasteInto(el, "https://example.com/docs\n");
    expect(paste.defaultPrevented).toBe(true);
    expect(area().value).toBe("see the [docs](https://example.com/docs) for it");
    expect(area().selectionStart).toBe("see the [docs](https://example.com/docs)".length);
    expect(commands).toEqual(["insertText"]);
  });

  it("leaves an ordinary paste with a selection to the textarea", () => {
    const el = mount(true, "see the docs for it");
    for (const text of [
      "the guide",
      "example.com/docs",
      "mailto:me@example.com",
      "https://a.com https://b.com",
    ]) {
      select(el, 8, 12);
      expect(pasteInto(el, text).defaultPrevented).toBe(false);
    }
    expect(area().value).toBe("see the docs for it");
  });

  it("leaves a URL pasted with nothing selected to the textarea", () => {
    const el = mount(true, "see ");
    select(el, 4);
    expect(pasteInto(el, "https://example.com").defaultPrevented).toBe(false);
    expect(area().value).toBe("see ");
  });
});

describe("the toolbar", () => {
  it("has the eight buttons, each with its name and shortcut in the tooltip", () => {
    mount(true);
    const tools = [...host.querySelectorAll(".comment-box-tools button")].map((b) =>
      b.getAttribute("aria-label"),
    );
    expect(tools).toEqual([
      "Bold",
      "Italic",
      "Code",
      "Link",
      "Bulleted list",
      "Numbered list",
      "Task list",
      "Quote",
    ]);
    expect(host.querySelector(".comment-box-tools")?.getAttribute("role")).toBe("toolbar");
  });

  const cases: [CommentFormat, string, string][] = [
    ["bold", "Bold", "fix it **now**"],
    ["italic", "Italic", "fix it _now_"],
    ["code", "Code", "fix it `now`"],
    ["link", "Link", "fix it [now](url)"],
    ["bullet_list", "Bulleted list", "- fix it now"],
    ["numbered_list", "Numbered list", "1. fix it now"],
    ["task_list", "Task list", "- [ ] fix it now"],
    ["quote", "Quote", "> fix it now"],
  ];
  it.each(cases)(
    "%s wraps the selection (or its line) and keeps the focus in the text",
    (_format, label, want) => {
      const el = mount(true, "fix it now");
      select(el, 7, 10);
      const down = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
      act(() => {
        tool(label).dispatchEvent(down);
        tool(label).click();
      });
      // The press doesn't take the focus from the text.
      expect(down.defaultPrevented).toBe(true);
      expect(area().value).toBe(want);
      expect(document.activeElement).toBe(area());
    },
  );

  it("inserts the marks with the caret between them when nothing is selected, and a second press takes them away", () => {
    const el = mount(true, "a ");
    select(el, 2);
    act(() => {
      tool("Bold").click();
    });
    expect(area().value).toBe("a ****");
    expect(area().selectionStart).toBe(4);
    act(() => {
      tool("Bold").click();
    });
    expect(area().value).toBe("a ");
  });
});

describe.each(PLATFORMS)("the shortcuts on $name", ({ mac, mods }) => {
  it("⌘B, ⌘I, ⌘E and ⌘K format the selection in the box; the app's actions on them don't run", () => {
    for (const [letter, want] of [
      ["b", "fix **now**"],
      ["i", "fix _now_"],
      ["e", "fix `now`"],
      ["k", "fix [now](url)"],
    ] as const) {
      const el = mount(mac, "fix now");
      select(el, 4, 7);
      const event = press(letter, mods);
      expect(event.defaultPrevented).toBe(true);
      expect(area().value).toBe(want);
    }
    // ⌘K in the box is its Link: `url` is selected for typing over; neither the palette nor Activity ran.
    expect(selected(area())).toBe("url");
    expect(ran).toEqual([]);
  });

  it("⌘K outside the box is the command palette: on the page, on the Preview tab and on a toolbar button", () => {
    mount(mac, "fix now");
    press("k", mods);
    act(() => {
      tab("Preview").focus();
    });
    press("k", mods);
    act(() => {
      tab("Write").click();
      tool("Bold").focus();
    });
    press("k", mods);
    // A text field that isn't the comment box's: ⌘K is the palette there too, and ⌘B the user's Activity.
    const field = host.appendChild(document.createElement("input"));
    act(() => {
      field.focus();
    });
    press("k", mods);
    press("b", mods);
    expect(ran).toEqual([
      "command_palette",
      "command_palette",
      "command_palette",
      "command_palette",
      "open_activity",
    ]);
    expect(area().value).toBe("fix now");
  });

  it("the other platform's modifier, Shift or Alt with the letter, does nothing in the box", () => {
    const el = mount(mac, "fix now");
    select(el, 4, 7);
    press("b", mac ? { ctrlKey: true } : { metaKey: true });
    press("b", { ...mods, shiftKey: true });
    press("i", { ...mods, altKey: true });
    expect(area().value).toBe("fix now");
  });

  it("⌘↵ still sends the comment, and Tell still tells", () => {
    const el = mount(mac, "**done**");
    select(el, 8);
    press("Enter", mods);
    act(() => {
      [...host.querySelectorAll("button")].find((b) => b.textContent === "Tell Strong now")?.click();
    });
    expect(sent.map((s) => [s.how, s.value.text])).toEqual([
      ["comment", "**done**"],
      ["tell", "**done**"],
    ]);
  });

  it("⌘V / Ctrl+V reaches the box as a paste (decision 048 keeps Ctrl+V a paste on Windows)", () => {
    const el = mount(mac, "x");
    select(el, 1);
    const key = press("v", mods);
    expect(key.defaultPrevented).toBe(false);
    const paste = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(paste, "clipboardData", { value: { getData: () => "**pasted**", files: [] } });
    act(() => {
      area().dispatchEvent(paste);
    });
    // Text is left to the textarea's own paste, as before.
    expect(paste.defaultPrevented).toBe(false);
    expect(ran).toEqual([]);
  });

  it("⌘Z after a toolbar button undoes it through the textarea's own history", () => {
    // The browser's editing commands, as WebKit and WebView2 run them on the focused textarea: each insert is one step
    // of its undo history, and Undo (what ⌘Z does there) takes the last one back.
    const history: { text: string; start: number; end: number }[] = [];
    const commands: string[] = [];
    const setText = (el: HTMLTextAreaElement, text: string) => {
      Reflect.set(HTMLTextAreaElement.prototype, "value", text, el);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    };
    const editing = (name: string, _ui: boolean, value = "") => {
      const el = document.activeElement;
      if (!(el instanceof HTMLTextAreaElement)) return false;
      commands.push(name);
      const { selectionStart: start, selectionEnd: end } = el;
      if (name === "insertText") {
        history.push({ text: el.value, start, end });
        setText(el, el.value.slice(0, start) + value + el.value.slice(end));
        el.setSelectionRange(start + value.length, start + value.length);
        return true;
      }
      if (name === "undo") {
        const last = history.pop();
        if (!last) return false;
        setText(el, last.text);
        el.setSelectionRange(last.start, last.end);
        return true;
      }
      return false;
    };
    Object.defineProperty(document, "execCommand", { configurable: true, value: editing });
    const el = mount(mac, "fix now");
    select(el, 4, 7);
    act(() => {
      tool("Bold").click();
    });
    expect(area().value).toBe("fix **now**");
    expect(commands).toEqual(["insertText"]);
    // ⌘Z is neither the app's nor the box's: the dispatcher and the box leave it to the textarea.
    const undo = press("z", mods);
    expect(undo.defaultPrevented).toBe(false);
    act(() => {
      editing("undo", false);
    });
    expect(area().value).toBe("fix now");
    expect(selected(area())).toBe("now");
  });
});
