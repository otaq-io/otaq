import { describe, expect, it, vi } from "vitest";

import type { ActiveTheme } from "../generated/ActiveTheme";
import type { PageEvent } from "../generated/PageEvent";
import type { ThemeMode } from "../generated/ThemeMode";
import type { ThemeParams } from "../generated/ThemeParams";
import { createPageEvents } from "../service";
import { request } from "../tauri";
import { COLOR_TOKENS, FIRST_LAUNCH, THEME_KEYS } from "./generated/tokens";
import { ICON_COUNT, iconUrl } from "./icons";
import { followTheme, type OsAppearance, terminalColors, watchTheme } from "./theme";

vi.mock("../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowTheme: vi.fn() }));
const requestMock = vi.mocked(request);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const theme = (key: string, dark: boolean, mode: ThemeMode = "system"): ActiveTheme => ({
  key,
  name: key,
  dark,
  mode,
});

/** An OS appearance the test turns, as macOS's Appearance setting would. */
function fakeOs(dark: boolean) {
  const listeners = new Set<() => void>();
  const os: OsAppearance & { turn(dark: boolean): void; listening(): number } = {
    dark: () => dark,
    onChange: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    turn: (to) => {
      dark = to;
      for (const listener of listeners) listener();
    },
    listening: () => listeners.size,
  };
  return os;
}

function fakeEvents() {
  let send: (event: PageEvent) => void = () => undefined;
  const events = createPageEvents((onEvent) => {
    send = onEvent;
    return Promise.resolve();
  });
  return {
    events,
    send: (event: PageEvent) => {
      send(event);
    },
  };
}

describe("the active theme", () => {
  it("is asked with the OS appearance, and again on each settings change", async () => {
    requestMock.mockReset();
    const { events, send } = fakeEvents();
    requestMock
      .mockResolvedValueOnce(theme(FIRST_LAUNCH.dark, true))
      .mockResolvedValueOnce(theme("daylight", false));
    const os = fakeOs(true);
    const seen: string[] = [];
    const stop = watchTheme((t) => seen.push(t.key), os, events);
    await settle();
    expect(requestMock).toHaveBeenLastCalledWith("theme.active", { os_dark: true });

    send({ kind: "service", name: "ticket_changed", event: {} });
    send({ kind: "service", name: "settings_changed", event: {} });
    await settle();
    expect(requestMock).toHaveBeenCalledTimes(2);
    expect(seen).toEqual([FIRST_LAUNCH.dark, "daylight"]);
    stop();
  });

  it("is asked again the moment the OS turns dark, so Match system switches live", async () => {
    requestMock.mockReset();
    const { events } = fakeEvents();
    requestMock.mockImplementation((_method, params) => {
      const dark = (params as ThemeParams).os_dark;
      return Promise.resolve(dark ? theme(FIRST_LAUNCH.dark, true) : theme(FIRST_LAUNCH.light, false));
    });
    const os = fakeOs(false);
    const root = { dataset: {} as DOMStringMap };
    const setWindow = vi.fn(() => Promise.resolve());
    const stop = followTheme(root, () => undefined, os, events, setWindow);
    await settle();
    expect(root.dataset.theme).toBe(FIRST_LAUNCH.light);

    os.turn(true);
    await settle();
    expect(requestMock).toHaveBeenLastCalledWith("theme.active", { os_dark: true });
    expect(root.dataset.theme).toBe(FIRST_LAUNCH.dark);
    // The window's own parts keep following the OS.
    expect(setWindow).toHaveBeenLastCalledWith("system");

    os.turn(false);
    await settle();
    expect(root.dataset.theme).toBe(FIRST_LAUNCH.light);
    stop();
    expect(os.listening()).toBe(0);
    requestMock.mockReset();
  });

  it("keeps only the newest answer when the OS turns while one is on its way", async () => {
    requestMock.mockReset();
    const { events } = fakeEvents();
    let answerFirst: (t: ActiveTheme) => void = () => undefined;
    requestMock
      .mockReturnValueOnce(
        new Promise((resolve) => {
          answerFirst = resolve;
        }),
      )
      .mockResolvedValueOnce(theme(FIRST_LAUNCH.dark, true));
    const os = fakeOs(false);
    const seen: string[] = [];
    const stop = watchTheme((t) => seen.push(t.key), os, events);
    await settle();
    os.turn(true);
    await settle();
    answerFirst(theme(FIRST_LAUNCH.light, false));
    await settle();
    expect(seen).toEqual([FIRST_LAUNCH.dark]);
    stop();
  });
});

describe("the page's theme", () => {
  it("starts on the OS's own Otaq theme and recolors on each settings change, without a reload", async () => {
    requestMock.mockReset();
    const { events, send } = fakeEvents();
    requestMock
      .mockResolvedValueOnce(theme(FIRST_LAUNCH.light, false))
      .mockResolvedValueOnce(theme("tokyo-night", true, "dark"));
    const root = { dataset: {} as DOMStringMap };
    const setWindow = vi.fn(() => Promise.resolve());
    const stop = followTheme(root, () => undefined, fakeOs(false), events, setWindow);
    expect(root.dataset.theme).toBe(FIRST_LAUNCH.light);
    await settle();
    expect(root.dataset.theme).toBe(FIRST_LAUNCH.light);
    expect(setWindow).toHaveBeenCalledWith("system");

    send({ kind: "service", name: "settings_changed", event: {} });
    await settle();
    expect(root.dataset.theme).toBe("tokyo-night");
    // Dark mode: the window's own parts are drawn dark too.
    expect(setWindow).toHaveBeenLastCalledWith("dark");
    expect(setWindow).toHaveBeenCalledTimes(2);
    stop();
  });
});

describe("the tokens", () => {
  it("list the 49 colours and the 15 themes", () => {
    expect(COLOR_TOKENS).toHaveLength(49);
    expect(THEME_KEYS).toHaveLength(15);
    expect(THEME_KEYS).toContain(FIRST_LAUNCH.dark);
    expect(THEME_KEYS).toContain(FIRST_LAUNCH.light);
  });

  it("give a terminal its colours by xterm.js's names", () => {
    const vars: Record<string, string> = {
      "--term-bg": " #0a0b0d",
      "--term-fg": "#d6d8dd",
      "--ansi-9": "#ff8a8f",
    };
    const colors = terminalColors({ getPropertyValue: (name) => vars[name] ?? "" });
    expect(colors.background).toBe("#0a0b0d");
    expect(colors.cursor).toBe("#d6d8dd");
    expect(colors.brightRed).toBe("#ff8a8f");
    expect(colors.black).toBe("");
  });

  it("give every theme's selected text its terminal text colour on the selection tint, focused or not", async () => {
    // Read as text with Node's fs (the test runner hands CSS imports over empty; the page has no Node types).
    const fsModule = "node:fs";
    const fs = (await import(fsModule)) as { readFileSync(path: URL, encoding: "utf8"): string };
    const tokensCss = fs.readFileSync(new URL("generated/tokens.css", import.meta.url), "utf8");
    const themes = [...tokensCss.matchAll(/\[data-theme="([^"]+)"\] \{([^}]*)\}/g)];
    expect(themes.map((t) => t[1])).toEqual([...THEME_KEYS]);
    for (const [, key, body] of themes) {
      const vars = new Map([...(body ?? "").matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2]]));
      const colors = terminalColors({ getPropertyValue: (name) => vars.get(name) ?? "" });
      expect(colors.selectionForeground, key).toBe(vars.get("--term-fg"));
      expect(colors.selectionForeground, key).toMatch(/^#[0-9a-f]{6}$/);
      expect(colors.selectionBackground, key).toBe(vars.get("--selection"));
      expect(colors.selectionInactiveBackground, key).toBe(vars.get("--selection"));
      expect(colors.selectionInactiveBackground, key).toMatch(/^#[0-9a-f]{6,8}$/);
    }
  });
});

describe("the icons", () => {
  it("are the bundled SVG files, by their Lucide names", () => {
    expect(ICON_COUNT).toBeGreaterThanOrEqual(70);
    expect(iconUrl("bell")).toMatch(/bell/);
    expect(iconUrl("message-circle-question-mark")).toBeDefined();
  });
});
