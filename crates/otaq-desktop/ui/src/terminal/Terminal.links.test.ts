// @vitest-environment happy-dom
// The terminal component's links and copying (d0117 points 7 and 9, T-0484): plain URLs and OSC 8 links open through
// "open link" on ⌘-click only, the hint shows while ⌘ is held over a link, and a program's copy reaches the clipboard
// from live output, never from the replay (T-0510: through xterm.js's clipboard add-on).
import type { ILink, ILinkHandler, ILinkProvider } from "@xterm/xterm";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Keymap } from "../generated/Keymap";
import type { TerminalFeed } from "../generated/TerminalFeed";
import { keymap } from "../app/keyboard";
import { createRegistries, RegistriesContext } from "../app/registry";
import { encodeBase64 } from "./input";
import { Terminal } from "./Terminal";

const made = vi.hoisted(
  () =>
    [] as {
      options: { linkHandler?: ILinkHandler };
      providers: ILinkProvider[];
      osc: Map<number, (data: string) => boolean>;
    }[],
);
const feeds = vi.hoisted(() => [] as ((feed: TerminalFeed) => void)[]);
const requests = vi.hoisted(() => [] as [string, unknown][]);

const RANGE = { start: { x: 1, y: 1 }, end: { x: 6, y: 1 } };

// xterm.js needs a canvas: a stand-in that keeps its link providers (its own OSC 8 one first, as xterm.js does), runs
// OSC 52 sequences through the registered handler and hands what the terminal answers (`input`) to `onData`.
vi.mock("@xterm/xterm", () => {
  const disposable = () => ({ dispose: () => undefined });
  class Terminal {
    options: { linkHandler?: ILinkHandler } = {};
    unicode = { activeVersion: "6", register: () => undefined };
    cols = 80;
    rows = 24;
    element = undefined;
    textarea = undefined;
    buffer = { active: { baseY: 0, viewportY: 0 } };
    osc = new Map<number, (data: string) => boolean>();
    typed: ((data: string) => void)[] = [];
    _core = {
      _linkProviderService: {
        linkProviders: [
          {
            // xterm.js's OSC 8 provider: one link, activated through the link handler.
            provideLinks: (_y: number, callback: (links: ILink[]) => void) => {
              callback([
                {
                  text: "https://example.com/pr/12",
                  range: RANGE,
                  activate: (event: MouseEvent, text: string) =>
                    this.options.linkHandler?.activate(event, text, RANGE),
                },
              ]);
            },
          },
        ] as ILinkProvider[],
      },
    };
    constructor() {
      made.push({
        options: this.options,
        providers: this._core._linkProviderService.linkProviders,
        osc: this.osc,
      });
    }
    parser = {
      registerOscHandler: (code: number, handler: (data: string) => boolean) => {
        this.osc.set(code, handler);
        return { dispose: () => this.osc.delete(code) };
      },
    };
    registerLinkProvider = (provider: ILinkProvider) => {
      this._core._linkProviderService.linkProviders.push(provider);
      return disposable();
    };
    loadAddon = (addon: { activate?: (t: unknown) => void }) => {
      addon.activate?.(this);
    };
    onData = (listener: (data: string) => void) => {
      this.typed.push(listener);
      return disposable();
    };
    input = (data: string) => {
      for (const listener of this.typed) listener(data);
    };
    onBinary = disposable;
    onResize = disposable;
    onScroll = disposable;
    onLineFeed = disposable;
    onWriteParsed = disposable;
    attachCustomKeyEventHandler = () => undefined;
    open = () => undefined;
    reset = () => undefined;
    resize = () => undefined;
    write = (data: Uint8Array, done?: () => void) => {
      const text = new TextDecoder().decode(data);
      // eslint-disable-next-line no-control-regex -- OSC 52 is an escape sequence
      for (const m of text.matchAll(/\x1b\]52;([^\x07]*)\x07/g)) this.osc.get(52)?.(m[1] ?? "");
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
// The web links add-on: registers a provider with one plain URL, opened through the add-on's handler.
vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: class {
    constructor(private handler: (event: MouseEvent, uri: string) => void) {}
    activate(term: { registerLinkProvider: (p: ILinkProvider) => unknown }) {
      term.registerLinkProvider({
        provideLinks: (_y, callback) => {
          callback([
            {
              text: "https://example.com/docs",
              range: RANGE,
              activate: (event, text) => {
                this.handler(event, text);
              },
            },
          ]);
        },
      });
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
  request: (name: string, args: unknown) => {
    requests.push([name, args]);
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
const copied: string[] = [];
let clipboardReads = 0;

beforeEach(() => {
  if (!("fonts" in document)) {
    Object.defineProperty(document, "fonts", {
      value: { load: () => Promise.resolve([]) },
      configurable: true,
    });
  }
  Object.defineProperty(navigator, "clipboard", {
    value: {
      writeText: (text: string) => {
        copied.push(text);
        return Promise.resolve();
      },
      readText: () => {
        clipboardReads += 1;
        return Promise.resolve("never this");
      },
    },
    configurable: true,
  });
  keymap.set({ platform: "mac" } as Keymap);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  made.length = 0;
  feeds.length = 0;
  requests.length = 0;
  copied.length = 0;
  clipboardReads = 0;
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  keymap.set(undefined);
});

function mount() {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(Terminal, { session: "s1", label: "SHOP-19 · agent terminal" }),
      ),
    );
  });
  const term = made.at(-1);
  if (!term) throw new Error("no terminal made");
  return term;
}

function linkOf(provider: ILinkProvider | undefined): ILink {
  let found: ILink | undefined;
  provider?.provideLinks(1, (links) => {
    found = links?.[0];
  });
  if (!found) throw new Error("no link");
  return found;
}

const click = (metaKey: boolean) => new MouseEvent("mouseup", { metaKey, clientX: 20, clientY: 10 });
const opened = () => requests.filter(([name]) => name === "terminal.open_link").map(([, args]) => args);

describe("the terminal component's links", () => {
  it("open a plain URL and an OSC 8 link through open link on ⌘-click only", () => {
    const term = mount();
    const hidden = linkOf(term.providers[0]);
    const plain = linkOf(term.providers[1]);
    for (const link of [hidden, plain]) link.activate(click(false), link.text);
    expect(opened()).toEqual([]);
    for (const link of [hidden, plain]) link.activate(click(true), link.text);
    expect(opened()).toEqual([{ url: "https://example.com/pr/12" }, { url: "https://example.com/docs" }]);
  });

  it("show “⌘-click to open” while ⌘ is held over a link, and nothing without it", () => {
    const term = mount();
    const link = linkOf(term.providers[1]);
    act(() => {
      link.hover?.(click(false), link.text);
    });
    expect(container.querySelector(".terminal-link-hint")).toBeNull();
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Meta", metaKey: true }));
    });
    expect(container.querySelector(".terminal-link-hint")?.textContent).toBe("⌘-click to open");
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta" }));
    });
    expect(container.querySelector(".terminal-link-hint")).toBeNull();
  });
});

describe("the terminal component's copying from programs", () => {
  const bytes = (text: string) => encodeBase64(new TextEncoder().encode(text));
  const osc52 = (text: string) => bytes(`\x1b]52;c;${bytes(text)}\x07`);
  const sentInput = () =>
    requests
      .filter(([name]) => name === "terminal.input")
      .map(([, args]) => args as { data: string; reply: boolean });

  it("copies from live output, never from the replay, and answers a query with an empty clipboard", async () => {
    mount();
    act(() => {
      for (const f of feeds) f({ kind: "phase", phase: "live" });
      for (const f of feeds) f({ kind: "replay", data: osc52("old copy"), size: null });
    });
    act(() => {
      for (const f of feeds) f({ kind: "replay", data: bytes("\x1b]52;c;?\x07"), size: null });
    });
    expect(copied).toEqual([]);
    expect(sentInput()).toEqual([]);
    act(() => {
      for (const f of feeds) f({ kind: "live", data: osc52("new copy") });
      for (const f of feeds) f({ kind: "live", data: bytes("\x1b]52;c;?\x07") });
    });
    await act(() => Promise.resolve());
    expect(copied).toEqual(["new copy"]);
    expect(sentInput()).toEqual([{ session: "s1", data: bytes("\x1b]52;c;\x07"), reply: true }]);
    expect(clipboardReads).toBe(0);
  });
});
