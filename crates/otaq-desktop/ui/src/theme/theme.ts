// The active theme (UIT-07): the page's root element carries `data-theme="<key>"`, which picks one block of the
// generated `tokens.css`, so a new theme recolors everything at once without a reload. The theme comes from the Rust
// side (`theme.active`, given the OS appearance now) and is asked again on every `settings_changed` event, so a pick
// in Settings or in another window lands here too, and on every change of the OS appearance, so Match system follows
// the OS live. The window's own parts (title bar, menus) follow the mode. The frame mounts it with one line:
// `useTheme()`.
import { useLayoutEffect, useState } from "react";

import type { ActiveTheme } from "../generated/ActiveTheme";
import type { ThemeMode } from "../generated/ThemeMode";
import type { ThemeParams } from "../generated/ThemeParams";
import { type PageEvents, pageEvents } from "../service";
import { request, windowTheme } from "../tauri";
import { FIRST_LAUNCH } from "./generated/tokens";

/** The OS appearance as the page sees it: whether it is dark now, and a listener for its changes (returns the stop). */
export interface OsAppearance {
  dark(): boolean;
  onChange(listener: () => void): () => void;
}

const DARK_QUERY = "(prefers-color-scheme: dark)";

/** The OS appearance from the webview's `prefers-color-scheme`, which follows the OS while the window does (Match
 * system). */
export const osAppearance: OsAppearance = {
  dark: () => window.matchMedia(DARK_QUERY).matches,
  onChange: (listener) => {
    const query = window.matchMedia(DARK_QUERY);
    query.addEventListener("change", listener);
    return () => {
      query.removeEventListener("change", listener);
    };
  },
};

/** Whether the OS draws windows dark now. */
export function osIsDark(): boolean {
  return osAppearance.dark();
}

/**
 * Follows the active theme: once the event stream is open, then on each settings change and each change of the OS
 * appearance. Only the newest answer counts. Returns the stop.
 */
export function watchTheme(
  onChange: (theme: ActiveTheme) => void,
  os: OsAppearance = osAppearance,
  events: PageEvents = pageEvents,
): () => void {
  let stopped = false;
  let newest = 0;
  const fetch = () => {
    const mine = ++newest;
    const params: ThemeParams = { os_dark: os.dark() };
    (request("theme.active", params) as Promise<ActiveTheme>).then(
      (theme) => {
        if (!stopped && mine === newest) onChange(theme);
      },
      () => undefined,
    );
  };
  const listening = events.listen((event) => {
    if (event.kind === "resync" || (event.kind === "service" && event.name === "settings_changed")) fetch();
  });
  listening.ready.then(fetch, () => undefined);
  const stopOs = os.onChange(fetch);
  return () => {
    stopped = true;
    listening.stop();
    stopOs();
  };
}

/**
 * Keeps `root`'s `data-theme` on the active theme, which picks its block of `tokens.css`: the page recolors at once,
 * without a reload. The window's own parts are told the mode whenever it changes (`setWindow`). Until the first
 * answer, Otaq's own theme for the OS. Returns the stop.
 */
export function followTheme(
  root: Pick<HTMLElement, "dataset">,
  onChange: (theme: ActiveTheme) => void,
  os: OsAppearance = osAppearance,
  events: PageEvents = pageEvents,
  setWindow: (mode: ThemeMode) => Promise<void> = windowTheme,
): () => void {
  root.dataset.theme ??= os.dark() ? FIRST_LAUNCH.dark : FIRST_LAUNCH.light;
  let shown: string | undefined;
  return watchTheme(
    (active) => {
      root.dataset.theme = active.key;
      // Again when the theme changes too: the menu-bar icon's menu draws its dots for the window's appearance.
      const look = `${active.mode} ${active.key}`;
      if (look !== shown) {
        shown = look;
        setWindow(active.mode).catch(() => undefined);
      }
      onChange(active);
    },
    os,
    events,
  );
}

/**
 * Applies the active theme to the page's root element and returns it (`undefined` until it is known). Until then the
 * page shows Otaq's own theme for the OS, so the first frame isn't unstyled.
 */
export function useTheme(): ActiveTheme | undefined {
  const [theme, setTheme] = useState<ActiveTheme>();
  // Before the first paint, so the first frame isn't unstyled.
  useLayoutEffect(() => followTheme(document.documentElement, setTheme), []);
  return theme;
}

/** The terminal colours of the theme in use, named as xterm.js names them (its `ITheme`). */
export interface TerminalColors {
  background: string;
  foreground: string;
  cursor: string;
  selectionBackground: string;
  selectionInactiveBackground: string;
  selectionForeground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

/**
 * Reads a terminal's colours from the CSS variables (`--term-bg`, `--ansi-0` … `--ansi-15`), e.g. from
 * `getComputedStyle(element)`. A terminal reads them again when the theme changes (`useTheme`'s value).
 */
export function terminalColors(style: Pick<CSSStyleDeclaration, "getPropertyValue">): TerminalColors {
  const read = (name: string) => style.getPropertyValue(`--${name}`).trim();
  const ansi = (n: number) => read(`ansi-${String(n)}`);
  return {
    background: read("term-bg"),
    foreground: read("term-fg"),
    cursor: read("term-fg"),
    // Selected text keeps the terminal's text colour on the same tint, focused or not (d0117 point 2).
    selectionBackground: read("selection"),
    selectionInactiveBackground: read("selection"),
    selectionForeground: read("term-fg"),
    black: ansi(0),
    red: ansi(1),
    green: ansi(2),
    yellow: ansi(3),
    blue: ansi(4),
    magenta: ansi(5),
    cyan: ansi(6),
    white: ansi(7),
    brightBlack: ansi(8),
    brightRed: ansi(9),
    brightGreen: ansi(10),
    brightYellow: ansi(11),
    brightBlue: ansi(12),
    brightMagenta: ansi(13),
    brightCyan: ansi(14),
    brightWhite: ansi(15),
  };
}
