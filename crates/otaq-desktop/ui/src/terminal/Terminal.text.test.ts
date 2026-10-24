// @vitest-environment happy-dom
// The terminal types plain text (#52): xterm.js's helper textarea, which takes the keys, and the find field carry
// no autocorrect, no capitals and no spell check, so macOS offers no "Hi" for "hi" and `'--port'` stays as typed.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createRegistries, RegistriesContext } from "../app/registry";
import { Terminal } from "./Terminal";

// xterm.js needs a canvas: a stand-in that opens a bare textarea, without the attributes xterm.js sets itself.
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
    parser = { registerOscHandler: disposable };
    registerLinkProvider = disposable;
    loadAddon = () => undefined;
    onData = disposable;
    onBinary = disposable;
    onResize = disposable;
    onScroll = disposable;
    onLineFeed = disposable;
    onWriteParsed = disposable;
    attachCustomKeyEventHandler = () => undefined;
    open = (element: HTMLElement) => {
      this.element = element;
      this.textarea = document.createElement("textarea");
      this.textarea.className = "xterm-helper-textarea";
      element.append(this.textarea);
    };
    focus = () => undefined;
    reset = () => undefined;
    resize = () => undefined;
    write = () => undefined;
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
  request: () => Promise.resolve(null),
  attachTerminal: () => Promise.resolve(1),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let container: HTMLElement;

beforeEach(() => {
  Object.defineProperty(document, "fonts", {
    value: { load: () => Promise.resolve([]) },
    configurable: true,
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
});

describe("the terminal's typing", () => {
  it("xterm.js's helper textarea has no autocorrect, capitals or spell check once open", async () => {
    await act(async () => {
      root.render(
        createElement(
          RegistriesContext,
          { value: createRegistries() },
          createElement(Terminal, { session: "s1", label: "SHOP-19 · agent terminal" }),
        ),
      );
      // The font load opens xterm.js.
      await Promise.resolve();
    });
    const textarea = container.querySelector(".xterm-helper-textarea");
    expect(textarea).not.toBeNull();
    expect(textarea?.getAttribute("autocorrect")).toBe("off");
    expect(textarea?.getAttribute("autocapitalize")).toBe("off");
    expect(textarea?.getAttribute("spellcheck")).toBe("false");
  });
});
