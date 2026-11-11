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
    private selection = "";
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
      });
    }
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
    open = (element: HTMLElement) => {
      this.element = element;
    };
    focus = () => undefined;
    reset = () => undefined;
    resize = () => undefined;
    write = (_data: unknown, done?: () => void) => {
      done?.();
    };
    dispose = () => undefined;
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
