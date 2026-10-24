// @vitest-environment happy-dom
// How the terminal component draws (d0117 points 1, 2, 3 and 6, T-0481): the options it gives xterm.js, the same for
// both renderers, and Unicode 11 widths before the first write, the replay included.
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { TerminalFeed } from "../generated/TerminalFeed";
import type { TerminalRenderer } from "../generated/TerminalRenderer";
import { createRegistries, RegistriesContext } from "../app/registry";
import { encodeBase64 } from "./input";
import { Terminal } from "./Terminal";

interface Made {
  // As given to the constructor, and as they are now.
  options: Record<string, unknown>;
  current: Record<string, unknown>;
  addons: object[];
  // The Unicode version active at each write, and what was written.
  writes: { version: string; text: string }[];
}

const made = vi.hoisted(() => [] as Made[]);
const feeds = vi.hoisted(() => [] as ((feed: TerminalFeed) => void)[]);
const renderer = vi.hoisted((): { setting: TerminalRenderer } => ({ setting: "dom" }));

// xterm.js needs a canvas: a stand-in that records what the component asks of it.
vi.mock("@xterm/xterm", () => {
  const disposable = () => ({ dispose: () => undefined });
  class Terminal {
    record: Made;
    options: Record<string, unknown>;
    unicode = { activeVersion: "6" };
    cols = 80;
    rows = 24;
    element = undefined;
    textarea = undefined;
    buffer = { active: { baseY: 0, viewportY: 0 } };
    registerLinkProvider = () => ({ dispose: () => undefined });
    constructor(options: Record<string, unknown>) {
      this.options = { ...options };
      this.record = { options, current: this.options, addons: [], writes: [] };
      made.push(this.record);
    }
    loadAddon = (addon: object) => {
      this.record.addons.push(addon);
    };
    onData = disposable;
    onBinary = disposable;
    onResize = disposable;
    onScroll = disposable;
    onLineFeed = disposable;
    onWriteParsed = disposable;
    parser = { registerOscHandler: disposable };
    attachCustomKeyEventHandler = () => undefined;
    open = () => undefined;
    reset = () => undefined;
    resize = () => undefined;
    write = (data: Uint8Array) => {
      this.record.writes.push({ version: this.unicode.activeVersion, text: new TextDecoder().decode(data) });
    };
    dispose = () => undefined;
  }
  return { Terminal };
});
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class FitAddon {
    fit = () => undefined;
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
  watchView: (_load: unknown, _on: unknown, onChange: (s: { view: unknown }) => void) => {
    onChange({ view: { font_size: 13, renderer: renderer.setting } });
    return () => undefined;
  },
}));
vi.mock("../tauri", () => ({
  request: () => Promise.resolve(null),
  attachTerminal: (_session: string, onFeed: (feed: TerminalFeed) => void) => {
    feeds.push(onFeed);
    return Promise.resolve(1);
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Two themes' worth of the variables the terminal reads (the real values are the 15 themes in tokens.css), served as
// the computed style: the stand-in DOM doesn't pass CSS variables down.
const LIGHT: Record<string, string> = { "--term-fg": "#1b1d22", "--selection": "#4c57d633" };
const DARK: Record<string, string> = { "--term-fg": "#d6d8dd", "--selection": "#6e78f74d" };
let variables = LIGHT;
vi.spyOn(window, "getComputedStyle").mockImplementation(
  () => ({ getPropertyValue: (name: string) => variables[name] ?? "" }) as CSSStyleDeclaration,
);

let root: Root;
let container: HTMLElement;

beforeEach(() => {
  if (!("fonts" in document)) {
    Object.defineProperty(document, "fonts", {
      value: { load: () => Promise.resolve([]) },
      configurable: true,
    });
  }
  variables = LIGHT;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  made.length = 0;
  feeds.length = 0;
  renderer.setting = "dom";
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  delete document.documentElement.dataset.theme;
});

async function mount() {
  await act(async () => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(Terminal, { session: "s1", label: "SHOP-19 · agent terminal" }),
      ),
    );
    // The font load, then the renderer effect.
    await Promise.resolve();
  });
  const term = made.at(-1);
  if (!term) throw new Error("no terminal made");
  return term;
}

function feed(item: TerminalFeed) {
  act(() => {
    for (const f of feeds) f(item);
  });
}

describe("the terminal's drawing options", () => {
  it("set the contrast floor, Option-drag selection and the selected text's colours", async () => {
    const term = await mount();
    expect(term.options).toMatchObject({
      minimumContrastRatio: 4.5,
      macOptionClickForcesSelection: true,
      macOptionIsMeta: true,
      theme: { foreground: "#1b1d22", selectionForeground: "#1b1d22" },
    });
    const theme = term.options.theme as Record<string, string>;
    expect(theme.selectionBackground).toBe("#4c57d633");
    expect(theme.selectionInactiveBackground).toBe("#4c57d633");
  });

  it("follow a theme change, the selected text's colours included", async () => {
    const term = await mount();
    variables = DARK;
    document.documentElement.dataset.theme = "dark";
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    expect(term.current.theme).toMatchObject({
      foreground: "#d6d8dd",
      selectionForeground: "#d6d8dd",
      selectionBackground: "#6e78f74d",
      selectionInactiveBackground: "#6e78f74d",
    });
  });

  it("are the same for the WebGL and the DOM renderer; WebGL only adds its add-on", async () => {
    renderer.setting = "dom";
    const dom = await mount();
    act(() => {
      root.unmount();
    });
    root = createRoot(container);
    renderer.setting = "webgl";
    const webgl = await mount();
    expect(webgl.options).toEqual(dom.options);
    expect(dom.addons.some((a) => a instanceof WebglAddon)).toBe(false);
    expect(webgl.addons.some((a) => a instanceof WebglAddon)).toBe(true);
  });

  it("count widths by Unicode 11 before the replay and every live write", async () => {
    const term = await mount();
    expect(term.addons[0]).toBeInstanceOf(Unicode11Addon);
    feed({ kind: "replay", data: encodeBase64(new TextEncoder().encode("✻ Thinking…")), size: null });
    feed({ kind: "live", data: encodeBase64(new TextEncoder().encode("⏺ Done 😀")) });
    expect(term.writes).toEqual([
      { version: "11", text: "✻ Thinking…" },
      { version: "11", text: "⏺ Done 😀" },
    ]);
  });
});

// T-0502 (dogfood #7): a wheel notch scrolls about 3 lines in WKWebView, a trackpad as far as the fingers (`wheel.ts`).
describe("the terminal's wheel", () => {
  function hostOf(): Element {
    const host = container.querySelector(".terminal-host");
    if (!host) throw new Error("no terminal host");
    return host;
  }

  function wheel(deltaY: number, host: Element = hostOf()) {
    // The xterm.js element inside the host, where the wheel lands.
    const inner = document.createElement("div");
    host.append(inner);
    inner.dispatchEvent(new WheelEvent("wheel", { deltaY, deltaMode: 0, bubbles: true }));
    inner.remove();
  }

  it("passes the scroll sensitivities to xterm.js", async () => {
    const term = await mount();
    expect(term.options).toMatchObject({ scrollSensitivity: 1, fastScrollSensitivity: 5 });
  });

  it("scrolls a notch's fractional pixels 12 times as far, a trackpad's whole pixels as they are", async () => {
    const term = await mount();
    const host = hostOf();
    // A slow notch: 0.1 line of 40 px, fixed-point.
    wheel(3.9996337890625);
    expect(term.current.scrollSensitivity).toBe(12);
    wheel(4);
    expect(term.current.scrollSensitivity).toBe(1);
    wheel(-4.000244140625);
    expect(term.current.scrollSensitivity).toBe(12);
    act(() => {
      root.unmount();
    });
    root = createRoot(container);
    // Unmounted, the old terminal no longer follows the wheel.
    wheel(4, host);
    expect(term.current.scrollSensitivity).toBe(12);
  });
});
