// @vitest-environment happy-dom
// A hidden terminal doesn't fit (T10: Noyan's terminal in the flow editor drawn ten columns wide): a closed panel keeps
// its terminal mounted, and fitting its hidden host read `width: 100%` as 100 px, squeezing the shared PTY. Shown
// again, it fits and tells the PTY its size.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { TerminalFeed } from "../generated/TerminalFeed";
import type { TerminalSettings } from "../generated/TerminalSettings";
import { createRegistries, RegistriesContext } from "../app/registry";
import { Terminal } from "./Terminal";

interface Made {
  current: Record<string, unknown>;
  // What the terminal was asked, in order: writes, resets, atlas clears.
  calls: string[];
}

const made = vi.hoisted(() => [] as Made[]);
const feeds = vi.hoisted(() => [] as ((feed: TerminalFeed) => void)[]);
const settings = vi.hoisted(() => ({ push: null as ((s: TerminalSettings) => void) | null }));
const loaded = vi.hoisted(() => [] as string[]);

vi.mock("@xterm/xterm", () => {
  const disposable = () => ({ dispose: () => undefined });
  class Terminal {
    record: Made;
    options: Record<string, unknown>;
    unicode = { activeVersion: "6" };
    cols = 80;
    rows = 24;
    element: unknown = undefined;
    textarea = undefined;
    buffer = { active: { baseY: 0, viewportY: 0 } };
    registerLinkProvider = () => ({ dispose: () => undefined });
    constructor(options: Record<string, unknown>) {
      this.options = { ...options };
      this.record = { current: this.options, calls: [] };
      made.push(this.record);
    }
    loadAddon = () => undefined;
    onData = disposable;
    onBinary = disposable;
    onResize = disposable;
    onScroll = disposable;
    onLineFeed = disposable;
    onWriteParsed = disposable;
    parser = { registerOscHandler: disposable };
    attachCustomKeyEventHandler = () => undefined;
    open = (parent: unknown) => {
      this.element = parent;
    };
    reset = () => {
      this.record.calls.push("reset");
    };
    resize = () => undefined;
    write = (data: Uint8Array) => {
      this.record.calls.push(`write ${new TextDecoder().decode(data)}`);
    };
    clearTextureAtlas = () => {
      this.record.calls.push(`atlas ${String(this.options.fontFamily)}`);
    };
    dispose = () => undefined;
  }
  return { Terminal };
});
const fits = vi.hoisted(() => ({ count: 0 }));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class FitAddon {
    fit = () => {
      fits.count++;
    };
  },
}));
vi.mock("@xterm/addon-search", () => ({
  SearchAddon: class SearchAddon {
    onDidChangeResults = () => ({ dispose: () => undefined });
  },
}));
vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: class WebLinksAddon {
    dispose = () => undefined;
  },
}));
vi.mock("@xterm/addon-webgl", () => ({
  WebglAddon: class WebglAddon {
    onContextLoss = () => undefined;
    dispose = () => undefined;
  },
}));
vi.mock("../service", () => ({
  watchView: (_load: unknown, _on: unknown, onChange: (s: { view: TerminalSettings }) => void) => {
    settings.push = (view) => {
      onChange({ view });
    };
    return () => undefined;
  },
}));
const sent = vi.hoisted(() => [] as [string, unknown][]);
vi.mock("../tauri", () => ({
  request: (name: string, params: unknown) => {
    sent.push([name, params]);
    return Promise.resolve(null);
  },
  attachTerminal: (_session: string, onFeed: (feed: TerminalFeed) => void) => {
    feeds.push(onFeed);
    return Promise.resolve(1);
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The ResizeObserver the terminal watches its host with, called by hand.
const observers = vi.hoisted(() => [] as (() => void)[]);
// Whether the host is laid out (happy-dom lays nothing out).
let shown = true;

let root: Root;
let container: HTMLElement;

beforeEach(() => {
  Object.defineProperty(document, "fonts", {
    value: {
      load: (font: string) => {
        loaded.push(font);
        return Promise.resolve([]);
      },
    },
    configurable: true,
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: () => void) {
        observers.push(callback);
      }
      observe = () => undefined;
      disconnect = () => undefined;
    },
  );
  vi.spyOn(Element.prototype, "getClientRects").mockImplementation(
    () => (shown ? [{ width: 300, height: 200 }] : []) as unknown as DOMRectList,
  );
  shown = true;
  fits.count = 0;
  sent.length = 0;
  observers.length = 0;
  made.length = 0;
  feeds.length = 0;
  loaded.length = 0;
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function settle() {
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
}

describe("a hidden terminal", () => {
  it("doesn't fit while its host is hidden, and fits and tells the PTY its size once shown again", async () => {
    act(() => {
      root.render(
        createElement(
          RegistriesContext,
          { value: createRegistries() },
          createElement(Terminal, { session: "s1", label: "Noyan" }),
        ),
      );
    });
    await settle();
    act(() => {
      for (const f of feeds) f({ kind: "phase", phase: "live" });
    });
    await settle();
    const fitted = fits.count;
    expect(fitted).toBeGreaterThan(0);
    const resizes = () => sent.filter(([name]) => name === "terminal.resize").length;
    const before = resizes();

    // The panel closes: no fit, nothing sent.
    shown = false;
    act(() => {
      for (const o of observers) o();
    });
    await settle();
    expect(fits.count).toBe(fitted);
    expect(resizes()).toBe(before);

    // It opens again: fitted, and the PTY told its size.
    shown = true;
    act(() => {
      for (const o of observers) o();
    });
    await settle();
    expect(fits.count).toBe(fitted + 1);
    expect(resizes()).toBe(before + 1);
  });
});
