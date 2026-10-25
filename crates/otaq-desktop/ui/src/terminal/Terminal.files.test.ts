// @vitest-environment happy-dom
// The terminal component's file paths (d0117 points 8 and 9, T-0485): a path Rust found in the output opens on
// ⌘-click only, where the terminal's host says: a ticket's terminal in the ticket's Files tab at the line, the Project
// agent in the project's Files view. A terminal with no place has no file links and asks nothing.
import type { ILink, ILinkHandler, ILinkProvider } from "@xterm/xterm";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Keymap } from "../generated/Keymap";
import { keymap } from "../app/keyboard";
import { createRegistries, RegistriesContext } from "../app/registry";
import type { FilesPlace } from "./files_place";
import { Terminal } from "./Terminal";

const made = vi.hoisted(
  () => [] as { options: { linkHandler?: ILinkHandler }; providers: ILinkProvider[] }[],
);
const requests = vi.hoisted(() => [] as [string, unknown][]);
const openFileIn = vi.hoisted(() => vi.fn());
const openProjectFile = vi.hoisted(() => vi.fn());

const OUTPUT = vi.hoisted(() => "Edited src/pricing.ts:42");

// xterm.js needs a canvas: a stand-in with one line of output and the link providers registered with it.
vi.mock("@xterm/xterm", () => {
  const disposable = () => ({ dispose: () => undefined });
  const line = {
    isWrapped: false,
    length: OUTPUT.length,
    getCell: (x: number) => ({ getChars: () => OUTPUT[x] ?? "", getWidth: () => 1 }),
  };
  class Terminal {
    options: { linkHandler?: ILinkHandler } = {};
    unicode = { activeVersion: "6", register: () => undefined };
    cols = 80;
    rows = 1;
    element = undefined;
    textarea = undefined;
    buffer = { active: { baseY: 0, viewportY: 0, getLine: (y: number) => (y === 0 ? line : undefined) } };
    _core = { _linkProviderService: { linkProviders: [] as ILinkProvider[] } };
    constructor() {
      made.push({ options: this.options, providers: this._core._linkProviderService.linkProviders });
    }
    parser = { registerOscHandler: disposable };
    registerLinkProvider = (provider: ILinkProvider) => {
      this._core._linkProviderService.linkProviders.push(provider);
      return disposable();
    };
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
    open = () => undefined;
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
vi.mock("../screens/files/FilesTab", () => ({ openFileIn }));
vi.mock("../screens/files_view/logic", () => ({ openProjectFile }));
vi.mock("../tauri", () => ({
  request: (name: string, args: { lines?: string[]; path?: string }) => {
    requests.push([name, args]);
    if (name === "file_links.find")
      return Promise.resolve(
        (args.lines ?? []).map((l) =>
          l === OUTPUT ? [{ start: 7, end: 24, path: "src/pricing.ts", line: 42 }] : [],
        ),
      );
    if (name === "file_links.check")
      return Promise.resolve(args.path === "/work/shop/src/pricing.ts" ? "src/pricing.ts" : null);
    return Promise.resolve(null);
  },
  attachTerminal: () => Promise.resolve(1),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let container: HTMLElement;

beforeEach(() => {
  if (!("fonts" in document)) {
    Object.defineProperty(document, "fonts", {
      value: { load: () => Promise.resolve([]) },
      configurable: true,
    });
  }
  keymap.set({ platform: "mac" } as Keymap);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  made.length = 0;
  requests.length = 0;
  openFileIn.mockClear();
  openProjectFile.mockClear();
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  keymap.set(undefined);
});

function mount(files?: FilesPlace) {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(Terminal, { session: "s1", label: "SHOP-19 · agent terminal", files }),
      ),
    );
  });
  const term = made.at(-1);
  if (!term) throw new Error("no terminal made");
  return term;
}

/** The file-path link on the line: the last provider registered. */
async function pathLink(providers: ILinkProvider[]): Promise<ILink | undefined> {
  const links = await new Promise<ILink[] | undefined>((resolve) => {
    providers.at(-1)?.provideLinks(1, resolve);
  });
  return links?.[0];
}

const click = (metaKey: boolean) => new MouseEvent("mouseup", { metaKey });

describe("the terminal component's file paths", () => {
  it("open a ticket's Files tab on the file at its line on ⌘-click, nothing on a plain click", async () => {
    const term = mount({ kind: "ticket", key: "SHOP-19" });
    const link = await pathLink(term.providers);
    expect(link?.text).toBe("src/pricing.ts:42");
    expect(requests).toContainEqual(["file_links.find", { session: "s1", lines: [OUTPUT] }]);
    link?.activate(click(false), link.text);
    expect(openFileIn).not.toHaveBeenCalled();
    link?.activate(click(true), link.text);
    expect(openFileIn).toHaveBeenCalledWith("SHOP-19", {
      path: "src/pricing.ts",
      child: null,
      mode: "file",
      line: 42,
    });
    expect(openProjectFile).not.toHaveBeenCalled();
  });

  it("open the project's Files view from Noyan", async () => {
    const term = mount({ kind: "project", project: "SHOP" });
    const link = await pathLink(term.providers);
    link?.activate(click(true), link.text);
    expect(openProjectFile).toHaveBeenCalledWith("SHOP", "src/pricing.ts", 42);
    expect(openFileIn).not.toHaveBeenCalled();
  });

  it("open a program's file: link inside the folder, checked by Rust", async () => {
    const term = mount({ kind: "ticket", key: "SHOP-19" });
    await act(async () => {
      term.options.linkHandler?.activate(click(true), "file://mac.local/work/shop/src/pricing.ts", {
        start: { x: 1, y: 1 },
        end: { x: 2, y: 1 },
      });
      await Promise.resolve();
    });
    expect(openFileIn).toHaveBeenCalledWith("SHOP-19", {
      path: "src/pricing.ts",
      child: null,
      mode: null,
      line: null,
    });
  });

  it("are no links without a place to open them, and nothing is asked", async () => {
    const term = mount();
    expect(await pathLink(term.providers)).toBeUndefined();
    expect(requests.filter(([name]) => name.startsWith("file_links."))).toEqual([]);
  });
});
