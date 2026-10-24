// The components' own interaction rules (UIT-04: React keeps the moment of interaction), kept apart from the
// drawing so Vitest tests them without a DOM. Wording and data rules stay in Rust; these only say which row is
// highlighted, which key does what, and where a floating layer goes.
import type { MenuItem } from "../generated/MenuItem";

/** Whether a menu row can be highlighted and chosen: not a header, not disabled (`menu.rs`, `MenuModel`). */
export function selectable(item: MenuItem | undefined): boolean {
  return item !== undefined && !item.disabled && !item.header;
}

/**
 * The next selectable row in `direction`, wrapping around; from nothing, down starts at the top and up at the
 * bottom. `null` when no row can be chosen. The same rule as `MenuModel::next` / `previous`.
 */
export function stepMenu(
  items: readonly MenuItem[],
  highlighted: number | null,
  direction: 1 | -1,
): number | null {
  const n = items.length;
  if (n === 0) return highlighted;
  const start = highlighted ?? (direction > 0 ? -1 : n);
  for (let k = 1; k <= n; k++) {
    const i = (((start + direction * k) % n) + n) % n;
    if (selectable(items[i])) return i;
  }
  return highlighted;
}

/** The id ↵ or a click on `index` chooses, if that row can be chosen (`MenuModel::activate`). */
export function chooseMenu(items: readonly MenuItem[], index: number | null): string | null {
  if (index === null) return null;
  const item = items[index];
  return item !== undefined && selectable(item) ? item.id : null;
}

/** What a key does in an open menu. */
export type MenuKeyResult =
  | { kind: "highlight"; index: number | null }
  | { kind: "choose"; id: string }
  | { kind: "close" }
  | { kind: "none" };

export function menuKey(items: readonly MenuItem[], highlighted: number | null, key: string): MenuKeyResult {
  switch (key) {
    case "ArrowDown":
      return { kind: "highlight", index: stepMenu(items, highlighted, 1) };
    case "ArrowUp":
      return { kind: "highlight", index: stepMenu(items, highlighted, -1) };
    case "Home":
      return { kind: "highlight", index: stepMenu(items, null, 1) };
    case "End":
      return { kind: "highlight", index: stepMenu(items, null, -1) };
    case "Enter":
    case " ": {
      const id = chooseMenu(items, highlighted);
      return id === null ? { kind: "none" } : { kind: "choose", id };
    }
    case "Escape":
    case "Tab":
      return { kind: "close" };
    default:
      return { kind: "none" };
  }
}

/**
 * The rows a searchable list shows for `query`: rows whose label or id holds it (any case), each header kept only
 * when a row under it is. An empty query keeps every row.
 */
export function filterMenu(items: readonly MenuItem[], query: string): readonly MenuItem[] {
  const q = query.trim().toLowerCase();
  if (q === "") return items;
  const out: MenuItem[] = [];
  let header: MenuItem | null = null;
  for (const item of items) {
    if (item.header) {
      header = item;
      continue;
    }
    if (!item.label.toLowerCase().includes(q) && !item.id.toLowerCase().includes(q)) continue;
    if (header !== null) {
      out.push(header);
      header = null;
    }
    out.push(item);
  }
  return out;
}

/**
 * What a key does in a searchable list, whose focus is in its search field: as in a menu, except that Space, Home
 * and End stay with the field (typing, moving the caret).
 */
export function searchKey(
  items: readonly MenuItem[],
  highlighted: number | null,
  key: string,
): MenuKeyResult {
  if (key === " " || key === "Home" || key === "End") return { kind: "none" };
  return menuKey(items, highlighted, key);
}

/** The row highlighted when a searchable list opens or its query changes: the checked row, else the first. */
export function firstMatch(items: readonly MenuItem[]): number | null {
  const checked = items.findIndex((i) => i.checked && selectable(i));
  return checked >= 0 ? checked : stepMenu(items, null, 1);
}

/** A dialog's two outcomes. */
export type DialogOutcome = "confirm" | "cancel";

/**
 * The default button, the one focused when the dialog opens and pressed by ↵: the confirm button of an ordinary
 * dialog, Cancel on a destructive one (`dialog.rs`: the primary is never pre-focused on destructive dialogs).
 */
export function defaultOutcome(destructive: boolean): DialogOutcome {
  return destructive ? "cancel" : "confirm";
}

/** What a key does in a dialog: Esc always cancels; ↵ presses the default button (`dialog.rs`, `on_key`). */
export function dialogKey(destructive: boolean, key: string): DialogOutcome | null {
  if (key === "Escape") return "cancel";
  if (key === "Enter") return defaultOutcome(destructive);
  return null;
}

/**
 * Where Tab moves focus inside a modal of `count` focusable elements: the next one (the previous with ⇧), wrapping
 * around, so focus never leaves the dialog. From outside the dialog (`current` -1) it enters at the first or last.
 */
export function trapFocus(count: number, current: number, back: boolean): number {
  if (count === 0) return -1;
  if (current < 0) return back ? count - 1 : 0;
  return (current + (back ? count - 1 : 1)) % count;
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Size {
  width: number;
  height: number;
}

/** The gap between an element and the menu, popover or tooltip under it. */
export const FLOAT_GAP = 4;
/** The least room kept between a floating layer and the window's edge. */
export const EDGE = 8;

/**
 * Where a floating layer of `size` goes next to `anchor`: under it, left edges aligned (`align: "end"`: right edges);
 * above it when there is no room below but there is above; always inside the window.
 */
export function placeFloating(
  anchor: Rect,
  size: Size,
  viewport: Size,
  align: "start" | "end" = "start",
): { left: number; top: number } {
  const below = anchor.top + anchor.height + FLOAT_GAP;
  const above = anchor.top - FLOAT_GAP - size.height;
  const fitsBelow = below + size.height <= viewport.height - EDGE;
  const top = fitsBelow || above < EDGE ? below : above;
  const wanted = align === "start" ? anchor.left : anchor.left + anchor.width - size.width;
  const left = Math.max(EDGE, Math.min(wanted, viewport.width - EDGE - size.width));
  return { left, top: Math.max(EDGE, Math.min(top, viewport.height - EDGE - size.height)) };
}

/** A press reaches a button's action only when it is neither disabled nor loading, so a double press can't start
 * an action twice. */
export function acceptsPress(disabled: boolean, loading: boolean): boolean {
  return !disabled && !loading;
}

/** The mark's viewBox side and its stroke widths in viewBox units at full size (*1. Otaq · Command tent*). */
export const MARK_VIEWBOX = 120;
export const MARK_OUTLINE = 6.5;
export const MARK_COLUMNS = 4.6;
/** The thinnest stroke the mark draws on screen, in px, so it still reads at 16–20 px. */
export const MARK_MIN_STROKE = 1.1;
/** How far the flag overlaps the pole, in viewBox units, so no seam shows between them. */
export const MARK_FLAG_OVERLAP = 0.16;

/**
 * The mark's stroke widths in viewBox units at `size` px: the design's widths, thickened below about 24 px so no
 * stroke drops under `MARK_MIN_STROKE` on screen. `flagShift` moves the flag right so it starts at the pole's edge.
 */
export function markStrokes(size: number): { outline: number; columns: number; flagShift: number } {
  const min = (MARK_MIN_STROKE * MARK_VIEWBOX) / size;
  const columns = Math.max(MARK_COLUMNS, min);
  return { outline: Math.max(MARK_OUTLINE, min), columns, flagShift: columns / 2 - MARK_FLAG_OVERLAP };
}

/**
 * A view model's text cut where it marks code with backticks ("For example `npm test`."): the odd pieces are code.
 * An unclosed backtick stays text.
 */
export function codeSpans(text: string): { text: string; code: boolean }[] {
  const parts = text.split("`");
  if (parts.length % 2 === 0) return [{ text, code: false }];
  return parts.flatMap((part, i) => (part === "" ? [] : [{ text: part, code: i % 2 === 1 }]));
}
