// The List's moment of interaction, without the DOM (LST-01, LST-02): its rows in order for ↑/↓, its groups that are
// drop targets (Group by Stage: a row dragged onto Ready queues it) for the drag, which is the Board's (`board/drag.ts`,
// the same `board.drag` / `board.drop`), and a lifted row's words. Everything a row says is Rust's (`list_view.rs`).
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

/** The groups a row can be dropped on, top to bottom, for ↑/↓ while a row is lifted with Space. */
export function listTargets(list: ListView): BoardDropTarget[] {
  return list.groups.flatMap((g) => (g.target === null ? [] : [g.target]));
}

/** Whether the List takes drags at all (Group by Stage). */
export function dragsRows(list: ListView): boolean {
  return list.groups.some((g) => g.target !== null);
}

/** The key ↑/↓ moves focus to from `key`; `null` at the ends. */
export function nextRow(keys: readonly string[], key: string, step: -1 | 1): string | null {
  const at = keys.indexOf(key);
  if (at === -1) return null;
  return keys[at + step] ?? null;
}

/**
 * The drop targets on screen while a row is dragged: each group with a target, from its head to its last row and
 * across the List's width (`ColumnSpan` with a top and bottom, which `columnAt` reads first).
 */
export function groupSpans(groups: ReadonlyMap<BoardDropTarget, HTMLElement>): ColumnSpan[] {
  return [...groups.entries()].map(([id, el]) => {
    const r = el.getBoundingClientRect();
    return { id, left: r.left, right: r.right, top: r.top, bottom: r.bottom };
  });
}
