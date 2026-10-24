// @vitest-environment happy-dom
// A terminal the user just asked for takes the keyboard as soon as it opens (d0126; findings 40 and 51 of the
// 2026-10-24 dogfood report): `focusWhenLive`, or `focus()` asked before xterm.js opened, is served right after
// `term.open`; what is typed before the session is live is sent, in order, once it is.
import { act, createElement, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Keymap } from "../generated/Keymap";
import type { TerminalFeed } from "../generated/TerminalFeed";
import { keymap } from "../app/keyboard";
import { createRegistries, RegistriesContext } from "../app/registry";
import { NOTHING_SELECTED, selection } from "../app/selection";
import { decodeBase64 } from "./input";
import { Terminal, type TerminalHandle, type TerminalProps } from "./Terminal";

interface Fake {
  type: (data: string) => void;
  focused: () => number;
  textarea: () => HTMLTextAreaElement | undefined;
  options: { disableStdin?: boolean };
}
const made = vi.hoisted(() => [] as Fake[]);
const sent = vi.hoisted(() => [] as [string, { data?: string; cols?: number }][]);
const feeds = vi.hoisted(() => [] as ((feed: TerminalFeed) => void)[]);
const font = vi.hoisted(() => ({ loaded: (): void => undefined }));

// xterm.js needs a canvas: a stand-in that opens a real textarea, focuses it and types what the test says.
vi.mock("@xterm/xterm", () => {
  const disposable = () => ({ dispose: () => undefined });
  class Terminal {
    options: { disableStdin?: boolean } = {};
    unicode = { activeVersion: "6", register: () => undefined };
    cols = 80;
    rows = 24;
    element: HTMLElement | undefined = undefined;
    textarea: HTMLTextAreaElement | undefined = undefined;
    buffer = { active: { baseY: 0, viewportY: 0, getLine: () => undefined } };
    _core = { _linkProviderService: { linkProviders: [] } };
    private onType: (data: string) => void = () => undefined;
    private focusCalls = 0;
    constructor(options: { disableStdin?: boolean }) {
      this.options = { ...options };
      made.push({
        // xterm.js sends nothing while stdin is disabled.
        type: (data) => {
          if (!this.options.disableStdin) this.onType(data);
        },
        focused: () => this.focusCalls,
        textarea: () => this.textarea,
        options: this.options,
      });
    }
    parser = { registerOscHandler: disposable };
    registerLinkProvider = disposable;
    loadAddon = (addon: { activate?: (t: unknown) => void }) => {
      addon.activate?.(this);
    };
    onData = (listener: (data: string) => void) => {
      this.onType = listener;
      return disposable();
    };
    onBinary = disposable;
    onResize = disposable;
    onScroll = disposable;
    onLineFeed = disposable;
    onWriteParsed = disposable;
    attachCustomKeyEventHandler = () => undefined;
    open = (element: HTMLElement) => {
      this.element = element;
      this.textarea = document.createElement("textarea");
      element.append(this.textarea);
    };
    focus = () => {
      this.focusCalls++;
      this.textarea?.focus();
    };
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
    activate() {
      return undefined;
    }
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
  request: (name: string, args: { data?: string; cols?: number }) => {
    sent.push([name, args]);
    return Promise.resolve(null);
  },
  attachTerminal: (_session: string, onFeed: (feed: TerminalFeed) => void) => {
    feeds.push(onFeed);
    return Promise.resolve(1);
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let container: HTMLElement;

beforeEach(() => {
  // The monospace font loads when the test says: until then xterm.js isn't open.
  Object.defineProperty(document, "fonts", {
    value: {
      load: () =>
        new Promise((resolve) => {
          font.loaded = () => {
            resolve([]);
          };
        }),
    },
    configurable: true,
  });
  keymap.set({ platform: "mac" } as Keymap);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  made.length = 0;
  sent.length = 0;
  feeds.length = 0;
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  keymap.set(undefined);
  selection.set(NOTHING_SELECTED);
});

function mount(props: Partial<TerminalProps> = {}) {
  const ref = createRef<TerminalHandle>();
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(Terminal, { session: "s1", label: "Terminal 2 · Terminals", ref, ...props }),
      ),
    );
  });
  const term = made.at(-1);
  if (!term) throw new Error("no terminal made");
  return { term, ref };
}

async function openFont() {
  await act(async () => {
    font.loaded();
    await Promise.resolve();
  });
}

function phase(next: TerminalFeed) {
  act(() => {
    for (const feed of feeds) feed(next);
  });
}

/** What reached the session as typing, in order. */
const typed = () =>
  sent
    .filter(([name]) => name === "terminal.input")
    .map(([, args]) => new TextDecoder().decode(decodeBase64(args.data ?? "")))
    .join("");

describe("a terminal that was asked for the keyboard", () => {
  it("takes it right after it opens with focusWhenLive, and says so", async () => {
    const { term } = mount({ focusWhenLive: true });
    expect(term.focused()).toBe(0);
    await openFont();
    expect(term.focused()).toBe(1);
    expect(document.activeElement).toBe(term.textarea());
    expect(selection.get().focusedTerminal).toBe("s1");
  });

  it("takes it after it opens when focus() came before (the drawer's +, the Pane, the sheet)", async () => {
    const { term, ref } = mount();
    act(() => {
      ref.current?.focus();
    });
    expect(term.focused()).toBe(0);
    await openFont();
    expect(term.focused()).toBe(1);
    expect(document.activeElement).toBe(term.textarea());
  });

  it("doesn't take it unasked", async () => {
    const { term } = mount();
    await openFont();
    expect(term.focused()).toBe(0);
  });

  it("sends what was typed before live once live, in order, and after it as usual", async () => {
    const { term } = mount({ focusWhenLive: true });
    await openFont();
    // Still attaching: the keys are taken, not sent yet.
    expect(term.options.disableStdin).toBe(false);
    term.type("ls");
    term.type(" -la");
    expect(typed()).toBe("");
    phase({ kind: "phase", phase: "live" });
    term.type("\r");
    await act(async () => {
      for (let i = 0; i < 5; i++) await Promise.resolve();
    });
    expect(typed()).toBe("ls -la\r");
    // The PTY gets the terminal's size before the typing.
    const first = sent.findIndex(([name]) => name === "terminal.input");
    expect(sent.findIndex(([name]) => name === "terminal.resize")).toBeLessThan(first);
  });

  it("drops what is held when the session ends before it was live", async () => {
    const { term } = mount();
    await openFont();
    term.type("echo hi");
    phase({ kind: "phase", phase: "ended" });
    phase({ kind: "phase", phase: "live" });
    await act(async () => {
      await Promise.resolve();
    });
    expect(typed()).toBe("");
  });
});
