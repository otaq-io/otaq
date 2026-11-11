// @vitest-environment happy-dom
// Ctrl+C in a terminal on Windows copies a selection, as in Windows Terminal: the selected text reaches the clipboard,
// the selection clears and the program gets nothing, so a draft in its input line stays (Codex clears it on Ctrl+C).
// With nothing selected Ctrl+C is the program's interrupt; Ctrl+Shift+C keeps copying; macOS doesn't change.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Keymap } from "../generated/Keymap";
import type { TerminalFeed } from "../generated/TerminalFeed";
import { keymap } from "../app/keyboard";
import { createRegistries, RegistriesContext } from "../app/registry";
import { decodeBase64 } from "./input";
import { Terminal } from "./Terminal";

interface Fake {
  /** A key press as xterm.js handles it: the custom handler first, then the key's bytes to the program unless the
   * handler took it. */
  press: (init: KeyboardEventInit, bytes: string) => void;
  select: (text: string) => void;
  selected: () => string;
  /** The mouse moves the program asked to hear (`term.modes.mouseTrackingMode`). */
  track: (mode: "none" | "drag" | "any") => void;
  /** A mouse move over the terminal's text (`screen`) or its scrollbar, with `buttons` held. */
  move: (over: "screen" | "scrollbar", buttons?: number) => void;
  /** The moves xterm.js's link hover heard on its screen. */
  hovers: () => number;
}
const made = vi.hoisted(() => [] as Fake[]);
const sent = vi.hoisted(() => [] as [string, { data?: string }][]);
const feeds = vi.hoisted(() => [] as ((feed: TerminalFeed) => void)[]);

vi.mock("@xterm/xterm", () => {
  const disposable = () => ({ dispose: () => undefined });
  class Terminal {
    options: Record<string, unknown> = {};
    unicode = { activeVersion: "6", register: () => undefined };
    cols = 80;
    rows = 24;
    element: HTMLElement | undefined = undefined;
    textarea: HTMLTextAreaElement | undefined = undefined;
    buffer = { active: { baseY: 0, viewportY: 0, getLine: () => undefined } };
    _core = { _linkProviderService: { linkProviders: [] } };
    modes = { mouseTrackingMode: "none" as "none" | "drag" | "any" };
    private selection = "";
    private hovered = 0;
    private text: HTMLElement | undefined = undefined;
    private scrollbar: HTMLElement | undefined = undefined;
    private onType: (data: string) => void = () => undefined;
    private onKey: (e: KeyboardEvent) => boolean = () => true;
    constructor() {
      made.push({
        press: (init, bytes) => {
          const e = new KeyboardEvent("keydown", { cancelable: true, ...init });
          if (this.onKey(e)) this.onType(bytes);
        },
        select: (text) => {
          this.selection = text;
        },
        selected: () => this.selection,
        track: (mode) => {
          this.modes.mouseTrackingMode = mode;
        },
        move: (over, buttons = 0) => {
          const target = over === "screen" ? this.text : this.scrollbar;
          target?.dispatchEvent(
            new MouseEvent("mousemove", { bubbles: true, buttons, clientX: 5, clientY: 5 }),
          );
        },
        hovers: () => this.hovered,
      });
    }
    /**
     * A mouse report as xterm.js 6 sends it: `CoreMouseService.triggerMouseEvent` hands it to
     * `CoreService.triggerDataEvent(report, true)`, which fires `onData` and then `onUserInput`, on which
     * `SelectionService` clears the selection. So a reported move clears a selection, as in the app.
     */
    private report = () => {
      this.onType("\x1b[<35;1;1M");
      this.selection = "";
    };
    /** xterm.js's `bindMouse`: buttonless moves on its element (`.xterm`), reported while the program tracks "any". */
    private plainMove = (e: MouseEvent) => {
      if (this.modes.mouseTrackingMode === "any" && !e.buttons) this.report();
    };
    /** xterm.js's `mousedrag`: moves with a button held, heard on the document, reported for "drag" and "any". */
    private drag = (e: MouseEvent) => {
      if (this.modes.mouseTrackingMode !== "none" && e.buttons) this.report();
    };
    hasSelection = () => this.selection !== "";
    getSelection = () => this.selection;
    clearSelection = () => {
      this.selection = "";
    };
    parser = { registerOscHandler: disposable };
    registerLinkProvider = disposable;
    loadAddon = () => undefined;
    onData = (listener: (data: string) => void) => {
      this.onType = listener;
      return disposable();
    };
    onBinary = disposable;
    onResize = disposable;
    onScroll = disposable;
    onLineFeed = disposable;
    onWriteParsed = disposable;
    attachCustomKeyEventHandler = (handler: (e: KeyboardEvent) => boolean) => {
      this.onKey = handler;
    };
    /** xterm.js's elements: `.xterm` in the host, holding its screen (where its link hover, `Linkifier`, listens on
     * mousemove) and the scrollbar beside it. */
    open = (parent: HTMLElement) => {
      const element = document.createElement("div");
      element.className = "terminal xterm";
      const screen = document.createElement("div");
      screen.className = "xterm-screen";
      const row = document.createElement("span");
      screen.append(row);
      const scrollbar = document.createElement("div");
      scrollbar.className = "scrollbar";
      element.append(screen, scrollbar);
      parent.append(element);
      screen.addEventListener("mousemove", () => {
        this.hovered++;
      });
      element.addEventListener("mousemove", this.plainMove);
      document.addEventListener("mousemove", this.drag);
      this.element = element;
      this.text = row;
      this.scrollbar = scrollbar;
    };
    focus = () => undefined;
    reset = () => undefined;
    resize = () => undefined;
    write = (_data: unknown, done?: () => void) => {
      done?.();
    };
    dispose = () => {
      document.removeEventListener("mousemove", this.drag);
    };
  }
  return { Terminal };
});
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit = () => undefined;
  },
}));
vi.mock("@xterm/addon-search", () => ({
  SearchAddon: class {
    onDidChangeResults = () => ({ dispose: () => undefined });
  },
}));
vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: class {
    dispose = () => undefined;
  },
}));
vi.mock("@xterm/addon-webgl", () => ({
  WebglAddon: class {
    dispose = () => undefined;
  },
}));
vi.mock("../service", () => ({ watchView: () => () => undefined }));
vi.mock("../tauri", () => ({
  request: (name: string, args: { data?: string }) => {
    sent.push([name, args]);
    return Promise.resolve(null);
  },
  attachTerminal: (_session: string, onFeed: (feed: TerminalFeed) => void) => {
    feeds.push(onFeed);
    return Promise.resolve(1);
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WINDOWS_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Edg/131.0.0.0";
const MAC_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const LINUX_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";

const NONE = { control: false, alt: false, shift: false, platform: false, function: false };
/** Windows and Linux: the terminal's copy and paste on Ctrl+Shift+C / V, as the Rust map sends them. */
const WIN_LINUX_MAP = {
  platform: "win_linux",
  terminal_keys: [
    {
      key: "copy",
      keystroke: { key: "c", code: "KeyC", modifiers: { ...NONE, control: true, shift: true } },
    },
    {
      key: "paste",
      keystroke: { key: "v", code: "KeyV", modifiers: { ...NONE, control: true, shift: true } },
    },
  ],
} as unknown as Keymap;
/** macOS: ⌘C is the menu's copy; the terminal's own keys have no Ctrl+C. */
const MAC_MAP = { platform: "mac", terminal_keys: [] } as unknown as Keymap;

const CTRL_C: KeyboardEventInit = { key: "c", code: "KeyC", ctrlKey: true };
const CTRL_SHIFT_C: KeyboardEventInit = { key: "C", code: "KeyC", ctrlKey: true, shiftKey: true };

let root: Root;
let container: HTMLElement;
const copied: string[] = [];

function platform(userAgent: string, map: Keymap) {
  Object.defineProperty(navigator, "userAgent", { value: userAgent, configurable: true });
  keymap.set(map);
}

beforeEach(() => {
  Object.defineProperty(document, "fonts", {
    value: { load: () => Promise.resolve([]) },
    configurable: true,
  });
  Object.defineProperty(navigator, "clipboard", {
    value: {
      writeText: (text: string) => {
        copied.push(text);
        return Promise.resolve();
      },
      readText: () => Promise.resolve(""),
    },
    configurable: true,
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  made.length = 0;
  sent.length = 0;
  feeds.length = 0;
  copied.length = 0;
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  keymap.set(undefined);
  Reflect.deleteProperty(navigator, "userAgent");
});

/** A live terminal, its xterm.js opened. */
async function live() {
  await act(async () => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(Terminal, { session: "s1", label: "SHOP-19 · agent terminal" }),
      ),
    );
    await Promise.resolve();
  });
  act(() => {
    for (const feed of feeds) feed({ kind: "phase", phase: "live" });
  });
  const term = made.at(-1);
  if (!term) throw new Error("no terminal made");
  return term;
}

async function settle() {
  await act(async () => {
    for (let i = 0; i < 5; i++) await Promise.resolve();
  });
}

/** What reached the program as typing, in order. */
const typed = () =>
  sent
    .filter(([name]) => name === "terminal.input")
    .map(([, args]) => new TextDecoder().decode(decodeBase64(args.data ?? "")))
    .join("");

describe("Ctrl+C in a terminal on Windows", () => {
  it("copies the selection, clears it and sends the program nothing", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.select("cargo test --workspace");
    term.press(CTRL_C, "\x03");
    await settle();
    expect(copied).toEqual(["cargo test --workspace"]);
    expect(term.selected()).toBe("");
    expect(typed()).toBe("");
  });

  it("sends the interrupt with nothing selected, and copies nothing", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.press(CTRL_C, "\x03");
    await settle();
    expect(copied).toEqual([]);
    expect(typed()).toBe("\x03");
  });

  it("a second Ctrl+C after the copy is the program's: the selection is gone", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.select("draft");
    term.press(CTRL_C, "\x03");
    term.press(CTRL_C, "\x03");
    await settle();
    expect(copied).toEqual(["draft"]);
    expect(typed()).toBe("\x03");
  });

  it("keeps Ctrl+Shift+C as copy, and Ctrl+Alt+C or Ctrl+X with a selection go to the program", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.select("ls -la");
    term.press(CTRL_SHIFT_C, "\x03");
    term.press({ ...CTRL_C, altKey: true }, "\x1b\x03");
    term.press({ key: "x", code: "KeyX", ctrlKey: true }, "\x18");
    await settle();
    expect(copied).toEqual(["ls -la"]);
    expect(typed()).toBe("\x1b\x03\x18");
  });
});

describe("Ctrl+C in a terminal elsewhere", () => {
  it("on macOS goes to the program even with a selection", async () => {
    platform(MAC_UA, MAC_MAP);
    const term = await live();
    term.select("ls -la");
    term.press(CTRL_C, "\x03");
    await settle();
    expect(copied).toEqual([]);
    expect(term.selected()).toBe("ls -la");
    expect(typed()).toBe("\x03");
  });

  it("on Linux goes to the program even with a selection; Ctrl+Shift+C copies", async () => {
    platform(LINUX_UA, WIN_LINUX_MAP);
    const term = await live();
    term.select("ls -la");
    term.press(CTRL_C, "\x03");
    term.press(CTRL_SHIFT_C, "\x03");
    await settle();
    expect(copied).toEqual(["ls -la"]);
    expect(typed()).toBe("\x03");
  });
});

// Over a program that hears every mouse move (Codex on Windows: its capture becomes DECSET 1003 through ConPTY),
// xterm.js reports each buttonless move as typing and so clears the selection; the stand-in does the same.
const REPORT = "\x1b[<35;1;1M";

describe("a selection over a program that hears every mouse move", () => {
  it("on Windows survives moving the mouse away, and Ctrl+C copies it with nothing sent", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.track("any");
    term.select("cargo test --workspace");
    term.move("screen");
    term.move("scrollbar");
    expect(term.selected()).toBe("cargo test --workspace");
    term.press(CTRL_C, "\x03");
    await settle();
    expect(copied).toEqual(["cargo test --workspace"]);
    expect(typed()).toBe("");
  });

  it("on Windows keeps Ctrl+Shift+C's copy after a move", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.track("any");
    term.select("ls -la");
    term.move("screen");
    term.press(CTRL_SHIFT_C, "\x03");
    await settle();
    expect(copied).toEqual(["ls -la"]);
    expect(typed()).toBe("");
  });

  it("on Windows the link hover still hears the kept move", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.track("any");
    term.select("https://example.com");
    term.move("screen");
    expect(term.hovers()).toBe(1);
  });

  it("on Windows reports moves with nothing selected, and drags", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.track("any");
    term.move("screen");
    term.select("draft");
    term.move("screen", 1);
    await settle();
    expect(term.hovers()).toBe(2);
    expect(typed()).toBe(REPORT + REPORT);
    expect(term.selected()).toBe("");
  });

  it("in a shell (no mouse tracking) a move leaves the selection for Ctrl+C", async () => {
    platform(WINDOWS_UA, WIN_LINUX_MAP);
    const term = await live();
    term.select("hello");
    term.move("screen");
    term.press(CTRL_C, "\x03");
    await settle();
    expect(copied).toEqual(["hello"]);
    expect(typed()).toBe("");
  });

  it("on macOS the move is reported as before", async () => {
    platform(MAC_UA, MAC_MAP);
    const term = await live();
    term.track("any");
    term.select("ls -la");
    term.move("screen");
    await settle();
    expect(typed()).toBe(REPORT);
    expect(term.selected()).toBe("");
  });
});
