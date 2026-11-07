// The List's moment of interaction, without the DOM: its rows in order for ↑/↓, its groups as their columns' drop
// targets for the drag, which is the Board's (`board/drag.ts`, the same `board.drag` / `board.drop`), and a lifted
// row's words. Everything a row says is Rust's (`list_view.rs`).
import type { BoardDropTarget } from "../../generated/BoardDropTarget";
import type { ListGroup } from "../../generated/ListGroup";
import type { ListRow } from "../../generated/ListRow";
import type { ListView } from "../../generated/ListView";
import type { ColumnSpan } from "../board/drag";

/** A group's ticket rows, top to bottom. */
export function rowsOf(group: ListGroup): ListRow[] {
  return group.entries.map((entry) => entry.row);
}

/** Every row's key, top to bottom: what ↑/↓ walk. */
export function rowKeys(list: ListView): string[] {
  return list.groups.flatMap((g) => rowsOf(g).map((r) => r.card.key));
}

/** The row of `key` and its group, if it is listed. */
export function findRow(list: ListView, key: string): { row: ListRow; group: ListGroup } | null {
  for (const group of list.groups) {
    const row = rowsOf(group).find((r) => r.card.key === key);
    if (row) return { row, group };
  }
  return null;
}

/** The drop targets top to bottom (every group, then the Done line), for ↑/↓ while a row is lifted with Space. */
export function listTargets(list: ListView): BoardDropTarget[] {
  return [...list.groups.map((g) => g.target), ...(list.done === null ? [] : [list.done.target])];
}

/** The key ↑/↓ moves focus to from `key`; `null` at the ends. */
export function nextRow(keys: readonly string[], key: string, step: -1 | 1): string | null {
  const at = keys.indexOf(key);
  if (at === -1) return null;
  return keys[at + step] ?? null;
}

/**
 * The drop targets on screen while a row is dragged: each group from its head to its last row, and the Done line,
 * across the List's width (`ColumnSpan` with a top and bottom, which `columnAt` reads first).
 */
export function groupSpans(groups: ReadonlyMap<BoardDropTarget, HTMLElement>): ColumnSpan[] {
  return [...groups.entries()].map(([id, el]) => {
    const r = el.getBoundingClientRect();
    return { id, left: r.left, right: r.right, top: r.top, bottom: r.bottom };
  });
}
