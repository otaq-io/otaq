// @vitest-environment happy-dom
// The terminal font (d0117 point 5, T-0482): a settings change reaches an open terminal at once, keeps what it shows
// and draws its glyphs again with the new font.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { TerminalFeed } from "../generated/TerminalFeed";
import type { TerminalSettings } from "../generated/TerminalSettings";
import { createRegistries, RegistriesContext } from "../app/registry";
import { encodeBase64 } from "./input";
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
    element = undefined;
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
    open = () => undefined;
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
  watchView: (_load: unknown, _on: unknown, onChange: (s: { view: TerminalSettings }) => void) => {
    settings.push = (view) => {
      onChange({ view });
    };
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

const GEIST = '"Geist Mono", "MesloLGS NF", monospace';
const MESLO = '"MesloLGS NF", monospace';

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
  made.length = 0;
  feeds.length = 0;
  loaded.length = 0;
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
});

async function settle() {
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
}

function push(fontFamily: string) {
  act(() => {
    settings.push?.({ font_size: 13, font_family: fontFamily, renderer: "dom" });
  });
}

describe("the terminal font", () => {
  it("follows a settings change on an open terminal, keeping its scrollback and redrawing its glyphs", async () => {
    act(() => {
      root.render(
        createElement(
          RegistriesContext,
          { value: createRegistries() },
          createElement(Terminal, { session: "s1", label: "SHOP-19 · agent terminal" }),
        ),
      );
    });
    push(GEIST);
    await settle();
    const term = made.at(-1);
    if (!term) throw new Error("no terminal made");
    expect(term.current.fontFamily).toBe(GEIST);

    act(() => {
      for (const f of feeds)
        f({ kind: "replay", data: encodeBase64(new TextEncoder().encode("p10k ")), size: null });
    });
    push(MESLO);
    await settle();
    expect(made).toHaveLength(1);
    expect(term.current.fontFamily).toBe(MESLO);
    expect(loaded.at(-1)).toBe(`13px ${MESLO}`);
    // The replay starts from a reset; the font change adds none and writes nothing again, so the scrollback stays.
    // The shared glyph atlas is never cleared: that scrambles the other terminals drawn with it.
    expect(term.calls).toEqual(["reset", "write p10k "]);
  });
});
