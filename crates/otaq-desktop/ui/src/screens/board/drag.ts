// The board's moment of interaction, without the DOM (UIT-04): a drag asks Rust once, when it starts (`board.drag`,
// every drop target's verdict from the core's `check_move`), and from then on only this file works while the pointer
// or the arrow keys move: which drop target is under the card, how it looks, whether a release drops. The targets
// are the view's (STG-08): every column, and a section with a `target` of its own after it (d0087: on the default
// flow the Review column, which means Ready for Review, and its Approved section). The verdicts and their words are
// Rust's; nothing here decides a rule.
import type { BoardColumn } from "../../generated/BoardColumn";
import type { BoardColumnId } from "../../generated/BoardColumnId";
import type { BoardDropTarget } from "../../generated/BoardDropTarget";
import type { BoardScrollChip } from "../../generated/BoardScrollChip";
import type { BoardTicketParams } from "../../generated/BoardTicketParams";
import type { DragAnswer } from "../../generated/DragAnswer";
import type { DragVerdict } from "../../generated/DragVerdict";

/** The two columns every flow has before its stages, by their fixed ids (STG-08, d0108): Backlog, and Ready for Dev,
 * the wait before the first Agent stage. */
export const BACKLOG: BoardColumnId = "backlog";
export const READY_FOR_DEV: BoardColumnId = "ready_for_dev";

/** The door's request call, or a fake one in the tests. */
export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/** How far the pointer moves with the button down before a press becomes a drag (a shorter press is a click). */
export const DRAG_THRESHOLD = 4;

/**
 * A drop target's place on screen, measured when the drag starts: a column's left and right edge, or a part of a
 * column (Review's Approved section) with its top and bottom too.
 */
export interface ColumnSpan {
  id: BoardDropTarget;
  left: number;
  right: number;
  top?: number;
  bottom?: number;
}

/** The drop target under the pointer: a section it is inside, else the column under `x`; the gaps belong to none. */
export function columnAt(spans: readonly ColumnSpan[], x: number, y = 0): BoardDropTarget | null {
  const inside = (s: ColumnSpan) => x >= s.left && x < s.right;
  const part = spans.find(
    (s) => s.top !== undefined && s.bottom !== undefined && inside(s) && y >= s.top && y < s.bottom,
  );
  return part?.id ?? spans.find((s) => s.top === undefined && inside(s))?.id ?? null;
}

/** The drop targets in board order, for ←/→ while a card is lifted: a column, then its sections with a target of
 * their own (Review's Approved section follows Review). */
export function targetOrder(columns: readonly Pick<BoardColumn, "id" | "sections">[]): BoardDropTarget[] {
  return columns.flatMap((c): BoardDropTarget[] => [
    c.id,
    ...c.sections.flatMap((s) => (s.target === null ? [] : [s.target])),
  ]);
}

/** How `target` looks for the dragged card; `undefined` until Rust's answer is in, or over no target. */
export function verdictOf(
  answer: DragAnswer | null,
  target: BoardDropTarget | null,
): DragVerdict | undefined {
  if (answer === null || target === null) return undefined;
  return answer.targets.find((t) => t.target === target)?.verdict;
}

/** The drag the board draws: the card, the column under it and Rust's answer once it came. */
export interface DragState {
  key: string;
  over: BoardDropTarget | null;
  answer: DragAnswer | null;
  /** Picked up with Space: ←/→ move it, Space drops. */
  keyboard: boolean;
}

/** What a release did. */
export type Release =
  /** Too short to be a drag: a click on the card. */
  | { kind: "click"; key: string }
  /** Dropped on a target whose verdict allows it: the board sends `board.drop`. */
  | { kind: "drop"; key: string; target: BoardDropTarget }
  /** Nothing: its own column, a forbidden one, no column, or Rust hadn't answered yet. The card flies back. */
  | { kind: "none" };

export interface DragCallbacks {
  /** The drag started, changed column, got its answer, or ended (`null`). Not called for every pointer move. */
  onState: (state: DragState | null) => void;
  onError?: (error: unknown) => void;
}

export interface Drag {
  /** The button went down on a card. With `clicks` false a short press is no click. */
  press: (key: string, x: number, y: number, clicks?: boolean) => void;
  /** The pointer moved; `spans` measures the drop targets once, when the press turns into a drag. Whether a pointer
   * drag is on (the caller then moves the lifted card to the pointer). */
  move: (x: number, y: number, spans: () => readonly ColumnSpan[]) => boolean;
  release: () => Release;
  /** Esc, or the pointer lost: the card flies back. */
  cancel: () => void;
  /** Space on a focused card in `column`: picks it up without the pointer. */
  lift: (key: string, column: BoardColumnId) => void;
  /** ←/→ while lifted: the next target in `order` (`targetOrder`), stopping at the ends. */
  step: (delta: -1 | 1, order: readonly BoardDropTarget[]) => void;
  state: () => DragState | null;
}

export function createDrag(ask: Ask, callbacks: DragCallbacks): Drag {
  let pressed: { key: string; x: number; y: number; clicks: boolean } | null = null;
  let spans: readonly ColumnSpan[] = [];
  let state: DragState | null = null;

  const set = (next: DragState | null) => {
    state = next;
    callbacks.onState(next);
  };

  const start = (key: string, over: BoardDropTarget | null, keyboard: boolean) => {
    set({ key, over, answer: null, keyboard });
    // The one request of the drag (UIT-04).
    const params: BoardTicketParams = { key };
    ask("board.drag", params).then(
      (answer) => {
        if (state?.key === key) set({ ...state, answer: answer as DragAnswer });
      },
      (error: unknown) => {
        callbacks.onError?.(error);
      },
    );
  };

  const end = (): Release => {
    const current = state;
    pressed = null;
    spans = [];
    if (current !== null) set(null);
    if (current === null || current.over === null) return { kind: "none" };
    return verdictOf(current.answer, current.over)?.kind === "allowed"
      ? { kind: "drop", key: current.key, target: current.over }
      : { kind: "none" };
  };

  return {
    press(key, x, y, clicks = true) {
      pressed = { key, x, y, clicks };
    },
    move(x, y, measure) {
      if (state === null) {
        if (pressed === null || Math.hypot(x - pressed.x, y - pressed.y) < DRAG_THRESHOLD) return false;
        spans = measure();
        start(pressed.key, columnAt(spans, x, y), false);
      }
      if (state === null || state.keyboard) return state !== null;
      const over = columnAt(spans, x, y);
      if (over !== state.over) set({ ...state, over });
      return true;
    },
    release() {
      if (state === null) {
        const click = pressed;
        pressed = null;
        return click?.clicks === true ? { kind: "click", key: click.key } : { kind: "none" };
      }
      return end();
    },
    cancel() {
      pressed = null;
      spans = [];
      if (state !== null) set(null);
    },
    lift(key, column) {
      pressed = null;
      start(key, column, true);
    },
    step(delta, order) {
      if (state === null) return;
      const at = state.over === null ? -1 : order.indexOf(state.over);
      const next = order[Math.min(order.length - 1, Math.max(0, at + delta))] ?? null;
      if (next !== state.over) set({ ...state, over: next });
    },
    state: () => state,
  };
}

/** `"{n} above"` with `n` filled in. */
export function fill(template: string, n: number): string {
  return template.replace("{n}", String(n));
}

export interface AboveChip {
  text: string;
  /** Some of them need the user: the turquoise chevron. */
  needYou: boolean;
}

/**
 * The chip of a scrolled column: "7 above · 6 need you", or "3 above"; `null` when no card is above the view.
 * `above` is, for each card scrolled out above, whether it needs the user.
 */
export function aboveChip(words: BoardScrollChip, above: readonly boolean[]): AboveChip | null {
  if (above.length === 0) return null;
  const needYou = above.filter(Boolean).length;
  const count = fill(words.above, above.length);
  return {
    text: needYou === 0 ? count : `${count} · ${fill(words.need_you, needYou)}`,
    needYou: needYou > 0,
  };
}

/** Where focus goes from card `row` of column `col` on an arrow key, over the columns' card keys; `null` stays. */
export function moveFocus(
  columns: readonly (readonly string[])[],
  col: number,
  row: number,
  key: "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight",
): { col: number; row: number } | null {
  if (key === "ArrowUp" || key === "ArrowDown") {
    const next = row + (key === "ArrowUp" ? -1 : 1);
    return next >= 0 && next < (columns[col]?.length ?? 0) ? { col, row: next } : null;
  }
  const step = key === "ArrowLeft" ? -1 : 1;
  for (let c = col + step; c >= 0 && c < columns.length; c += step) {
    const length = columns[c]?.length ?? 0;
    if (length > 0) return { col: c, row: Math.min(row, length - 1) };
  }
  return null;
}

/** A span on screen, left to right. */
export interface Span {
  left: number;
  right: number;
}

/**
 * The board's `scrollLeft` that shows `column` in `view`, the part of the board the sheet leaves uncovered (SHL-05):
 * a column that is already wholly in view stays (`null`); otherwise it goes to the view's left edge, which also
 * shows as much as fits of a column wider than the view.
 */
export function revealScroll(column: Span, view: Span, scrollLeft: number): number | null {
  if (column.left >= view.left && column.right <= view.right) return null;
  return Math.max(0, Math.round(scrollLeft + column.left - view.left));
}
