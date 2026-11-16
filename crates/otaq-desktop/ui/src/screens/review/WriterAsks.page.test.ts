// @vitest-environment happy-dom
// Answer in terminal (F-F-2): the writer's terminal opens with the keyboard in it, so the first key answers the
// prompt; Esc closes the overlay, back to the dialog under it, and never reaches the tool, where it would decline the
// prompt (decision 044).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Keymap } from "../../generated/Keymap";
import type { TerminalFeed } from "../../generated/TerminalFeed";
import { keymap } from "../../app/keyboard";
import { createRegistries, RegistriesContext } from "../../app/registry";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import { Dialog } from "../../components";
import { AnswerInTerminal } from "./WriterAsks";

const textareas = vi.hoisted(() => [] as HTMLTextAreaElement[]);
/** The keys xterm.js took for the tool. */
const toTool = vi.hoisted(() => [] as string[]);
const feeds = vi.hoisted(() => [] as ((feed: TerminalFeed) => void)[]);
const font = vi.hoisted(() => ({ loaded: (): void => undefined }));

// xterm.js needs a canvas: a stand-in that opens a real textarea which, as xterm.js does, takes every key it hears
// for the tool and stops it there.
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
    constructor(options: Record<string, unknown>) {
      this.options = { ...options };
    }
    parser = { registerOscHandler: disposable };
    registerLinkProvider = disposable;
    loadAddon = (addon: { activate?: (t: unknown) => void }) => {
      addon.activate?.(this);
    };
    onData = disposable;
    onBinary = disposable;
    onResize = disposable;
    onScroll = disposable;
    onLineFeed = disposable;
    onWriteParsed = disposable;
    attachCustomKeyEventHandler = () => undefined;
    open = (element: HTMLElement) => {
      this.element = element;
      const textarea = document.createElement("textarea");
      textarea.addEventListener("keydown", (e) => {
        toTool.push(e.key);
        e.preventDefault();
        e.stopPropagation();
      });
      this.textarea = textarea;
      textareas.push(textarea);
      element.append(textarea);
    };
    focus = () => {
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
vi.mock("../../service", () => ({ watchView: () => () => undefined }));
vi.mock("../../tauri", () => ({
  request: () => Promise.resolve(null),
  attachTerminal: (_session: string, onFeed: (feed: TerminalFeed) => void) => {
    feeds.push(onFeed);
    return Promise.resolve(1);
  },
  subscribeEvents: vi.fn(),
  windowAction: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let host: HTMLElement;

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
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  textareas.length = 0;
  toTool.length = 0;
  feeds.length = 0;
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  keymap.set(undefined);
  selection.set(NOTHING_SELECTED);
});

const ASKS = {
  session: "dev-SHOP-18",
  answer: "Answer in terminal",
  terminal_title: "SHOP-18 · Open PR",
};

/** The dialog Answer in terminal sits in (the Pushing… dialog), with the overlay opened over it and xterm.js open. */
async function openOverlay() {
  const cancelUnder = vi.fn();
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(Dialog, {
          title: "Pushing SHOP-18…",
          footer: null,
          onCancel: cancelUnder,
          children: createElement(AnswerInTerminal, { asks: ASKS }),
        }),
      ),
    );
  });
  const button = [...document.querySelectorAll("button")].find((b) => b.textContent === "Answer in terminal");
  expect(button).toBeDefined();
  act(() => {
    button?.click();
  });
  await act(async () => {
    font.loaded();
    await Promise.resolve();
  });
  return { cancelUnder };
}

const overlay = () => document.querySelector(".writer-terminal");

function press(target: EventTarget, key: string) {
  act(() => {
    target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  });
}

test("the terminal opens with the keyboard in it: the first key goes to the tool", async () => {
  await openOverlay();
  expect(overlay()).not.toBeNull();
  const textarea = textareas.at(-1);
  expect(textarea).toBeDefined();
  expect(document.activeElement).toBe(textarea);
  expect(selection.get().focusedTerminal).toBe("dev-SHOP-18");
  press(document.activeElement ?? document.body, "1");
  expect(toTool).toEqual(["1"]);
});

test("Esc in the terminal closes the overlay only and never reaches the tool", async () => {
  const { cancelUnder } = await openOverlay();
  const textarea = textareas.at(-1);
  expect(document.activeElement).toBe(textarea);
  press(textarea ?? document.body, "Escape");
  expect(toTool).toEqual([]);
  expect(overlay()).toBeNull();
  // The dialog under it stays open: Esc closed one layer.
  expect(cancelUnder).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("Answer in terminal");
});

test("Esc with the focus elsewhere in the overlay closes it too, as before", async () => {
  const { cancelUnder } = await openOverlay();
  const box = overlay()?.closest<HTMLElement>(".dialog");
  expect(box).toBeDefined();
  act(() => {
    box?.focus();
  });
  press(box ?? document.body, "Escape");
  expect(toTool).toEqual([]);
  expect(overlay()).toBeNull();
  expect(cancelUnder).not.toHaveBeenCalled();
});
