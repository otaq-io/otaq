// The terminal component's own state, as plain functions the tests drive without a real terminal: what it tells its
// host (`TerminalState`), the find bar's count, the font size, the renderer it asks xterm.js for, and its own keys.
import type { Keymap } from "../generated/Keymap";
import type { Keystroke } from "../generated/Keystroke";
import type { TerminalKey } from "../generated/TerminalKey";
import type { TerminalPhase } from "../generated/TerminalPhase";
import type { TerminalRenderer } from "../generated/TerminalRenderer";
import { type KeyLike, strokeMatches } from "../app/keys";

/** What a host shows around the terminal (the header and hint strips of T-0224), told on every change. */
export interface TerminalState {
  phase: TerminalPhase;
  /** The keyboard is in the terminal. */
  focused: boolean;
  /** How far the view is scrolled up from the newest line, in lines (0: following the output). */
  scrolledUp: number;
  /** Lines that arrived while scrolled up ("142 new lines"). */
  newLines: number;
  /** Points: the setting's, or what ⌘+ / ⌘− picked since. */
  fontSize: number;
}

export const INITIAL_STATE: TerminalState = {
  phase: "attaching",
  focused: false,
  scrolledUp: 0,
  newLines: 0,
  fontSize: 13,
};

/** Only a live session takes input; while reconnecting or after the end the scrollback stays readable (SVC-04). */
export function takesInput(phase: TerminalPhase): boolean {
  return phase === "live";
}

/**
 * What becomes of what the user types (d0126): sent while live; held while the terminal is still attaching and sent in
 * order once it is live (a terminal that was just asked for gets the keyboard before its session answers); dropped
 * while reconnecting and after the end.
 */
export function typingIn(phase: TerminalPhase): "send" | "hold" | "drop" {
  if (phase === "live") return "send";
  return phase === "attaching" ? "hold" : "drop";
}

/** The terminal font sizes (SET-01's range). */
export const FONT_SIZES = { min: 8, max: 32 } as const;

export function clampFontSize(size: number): number {
  return Math.min(FONT_SIZES.max, Math.max(FONT_SIZES.min, Math.round(size)));
}

/** A size picked with ⌘+ / ⌘−, and the setting it was picked from. */
export interface PickedSize {
  setting: number;
  size: number;
}

/** The size drawn: a picked one until the setting changes, else the setting's. */
export function fontSizeOf(setting: number | undefined, picked: PickedSize | undefined): number {
  const base = setting ?? INITIAL_STATE.fontSize;
  return clampFontSize(picked?.setting === base ? picked.size : base);
}

/** The find bar's count: "3 of 12"; xterm.js stops counting at its highlight limit ("1000+"). */
export function findCount(index: number, count: number, limit = 1000): string {
  if (count === 0) return "No results";
  const total = count >= limit ? `${String(limit)}+` : String(count);
  return index < 0 ? total : `${String(index + 1)} of ${total}`;
}

/** The pill over a scrolled-up terminal. */
export function jumpLabel(newLines: number): string {
  if (newLines <= 0) return "Jump to latest";
  return newLines === 1 ? "1 new line" : `${newLines.toLocaleString("en-US")} new lines`;
}

/** The renderer to start: the setting's, unless WebGL already failed in this terminal. */
export function rendererFor(setting: TerminalRenderer, webglFailed: boolean): TerminalRenderer {
  return setting === "webgl" && !webglFailed ? "webgl" : "dom";
}

/** The terminal's own key `e` presses (copy, paste, font size, new line), from the Rust map. */
export function terminalKey(map: Keymap | undefined, e: KeyLike): TerminalKey | undefined {
  return map?.terminal_keys.find((b) => strokeMatches(b.keystroke, e))?.key;
}

/** Ctrl+C alone, as the layout types C (Ctrl+Shift+C stays the terminal's copy key on Windows and Linux). */
const CTRL_C: Keystroke = {
  key: "c",
  code: "KeyC",
  modifiers: { control: true, alt: false, shift: false, platform: false, function: false },
};

/** Whether the page runs on Windows (WebView2 names it in its user agent; Linux's WebKitGTK and macOS don't). */
export function onWindows(userAgent: string = navigator.userAgent): boolean {
  return userAgent.includes("Windows");
}

/**
 * Whether Ctrl+C copies instead of reaching the program: on Windows, with text selected, as in Windows Terminal. The
 * program then gets nothing, so a draft in its input line stays (Codex clears it on Ctrl+C); with nothing selected
 * Ctrl+C is the program's (an interrupt). macOS and Linux keep their own keys.
 */
export function ctrlCCopies(windows: boolean, hasSelection: boolean, e: KeyLike): boolean {
  return windows && hasSelection && strokeMatches(CTRL_C, e);
}

/** What xterm.js says a program asked to hear of the mouse (`term.modes.mouseTrackingMode`). */
export type MouseTracking = "none" | "x10" | "vt200" | "drag" | "any";

/**
 * Whether a mouse move over the terminal is kept from the program so a selection stays until Ctrl+C copies it: on
 * Windows, with text selected, while the program asks for every mouse move ("any"; Codex's mouse capture becomes this
 * through ConPTY) and no button is held. xterm.js reports such a move to the program as typing, and typing clears the
 * selection, so the first move after a Shift-drag would lose it. Drags, programs that don't ask for plain moves (a
 * shell), and macOS and Linux are unchanged.
 */
export function keepsSelectionOnMove(
  windows: boolean,
  hasSelection: boolean,
  tracking: MouseTracking,
  buttons: number,
): boolean {
  return windows && hasSelection && tracking === "any" && buttons === 0;
}

/** What ⇧↵ (`new_line`) sends: ESC + CR, as ⌥↵, a new line in the prompt for every agent tool; xterm.js would send
 * `\r` (↵, which sends the prompt). */
export const NEW_LINE = "\x1b\r";

/** A theme colour as xterm.js's match decorations take it: `#RRGGBB`, without alpha. */
export function opaqueHex(color: string): string {
  const hex = color.trim();
  return /^#[0-9a-f]{8}$/i.test(hex) ? hex.slice(0, 7) : hex;
}
