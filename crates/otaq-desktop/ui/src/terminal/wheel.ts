// How far a mouse wheel scrolls a terminal (T-0502, dogfood #7): about 3 lines per notch, in every terminal, while a
// trackpad keeps following the fingers.
//
// Measured in WKWebView (a WKWebView fed NSEvents, macOS 27): a wheel notch arrives as a pixel `deltaY` of 40 per line
// macOS reports, and macOS's wheel acceleration makes a slow notch 0.1 line, so `deltaY` ≈ 4 (3.9996 or 4.0002: the
// line count is fixed-point). A trackpad's pixels arrive as they are, so 4 px of trackpad is also `deltaY` 4 (exactly):
// the event has no field that tells them apart, and one `scrollSensitivity` for both can't be right. At xterm.js's
// default of 1 a slow notch scrolls about 0.3 lines of the scrollback and a program that takes the wheel (Claude Code)
// gets one wheel step per ~13 notches (xterm.js treats a `deltaY` under 50 as a trackpad and takes 0.3 of it).
//
// So the sensitivity is set per wheel event, before xterm.js reads it: a fractional `deltaY` (a notch's fixed-point
// line count) scrolls `NOTCH_SENSITIVITY` times as far; whole pixels (a trackpad, or a fast spin of whole lines, which
// is already 40 px a line) keep xterm.js's 1.
import type { Terminal } from "@xterm/xterm";

/** xterm.js's own: whole-pixel wheels (trackpads) scroll as far as the fingers. */
export const SCROLL_SENSITIVITY = 1;

/**
 * A slow notch (`deltaY` ≈ 4) times this: about 3.4 lines of scrollback at the default 13 px font (xterm.js scrolls 50
 * px per 120 `wheelDelta`, so 4.6 px × 12 = 55 px over 16 px lines), and one wheel step per notch, nearly, for a
 * program that takes the wheel (4 × 12 × 0.3 = 14.4 px of a 16 px line, the rest carried to the next notch).
 */
export const NOTCH_SENSITIVITY = 12;

/** Option (and, in a program that takes the wheel, Shift or Control) with the wheel: five times as far, a screenful. */
export const FAST_SCROLL_SENSITIVITY = 5;

/** The sensitivity for one wheel event: `NOTCH_SENSITIVITY` for a notch's fractional pixels, else 1. */
export function wheelSensitivity(event: Pick<WheelEvent, "deltaY" | "deltaMode">): number {
  if (event.deltaMode !== 0 || event.deltaY === 0) return SCROLL_SENSITIVITY;
  const fraction = Math.abs(event.deltaY - Math.round(event.deltaY));
  return fraction > 1e-6 ? NOTCH_SENSITIVITY : SCROLL_SENSITIVITY;
}

/**
 * Sets `term`'s sensitivity for each wheel event over `element` (the terminal's box), in the capture phase so it is set
 * before xterm.js's own handlers read it, the scrollback's and the one that passes the wheel to a program. Returns its
 * removal.
 */
export function followWheel(element: HTMLElement, term: Pick<Terminal, "options">): () => void {
  const onWheel = (event: WheelEvent) => {
    const sensitivity = wheelSensitivity(event);
    if (term.options.scrollSensitivity !== sensitivity) term.options.scrollSensitivity = sensitivity;
  };
  element.addEventListener("wheel", onWheel, { capture: true, passive: true });
  return () => {
    element.removeEventListener("wheel", onWheel, { capture: true });
  };
}
