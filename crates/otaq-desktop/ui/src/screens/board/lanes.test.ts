import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { BoardCard } from "../../generated/BoardCard";
import type { BoardColumn } from "../../generated/BoardColumn";
import type { BoardLane } from "../../generated/BoardLane";
import type { DragAnswer } from "../../generated/DragAnswer";
import type { DropOutcome } from "../../generated/DropOutcome";
import { dropCard } from "./Board";
import { type Ask, createDrag, liftedLane } from "./drag";
import { laneColumns, Lanes, type LanesProps, laneWalk } from "./Lanes";

const column = (id: string, title: string, over: Partial<BoardColumn> = {}): BoardColumn => ({
  id,
  kind: "agent",
  title,
  icon: "bot",
  count: "0",
  quiet_icon: false,
  folded: [],
  tooltip: null,
  hint: null,
  empty_hint: "",
  collapsed: false,
  rail: null,
  menu: [],
  menu_label: "Column menu",
  hidden: null,
  cards: [],
  sections: [],
  folds: [],
  dev: null,
  add: null,
  ...over,
});

const COLUMNS: BoardColumn[] = [
  column("backlog", "Backlog", { kind: "backlog" }),
  column("ready_for_dev", "Ready for Dev", { kind: "ready_for_dev" }),
  column("dev", "Dev"),
  column("done", "Done", { kind: "done", collapsed: true }),
];

const cardOf = (key: string): BoardCard => ({
  kind: "ticket",
  needs_you: false,
  card: {
    key,
    key_label: key,
    epic: "Wishlist",
    priority: null,
    title: `${key} title`,
    status: null,
    blocks: null,
    avatar: null,
    who: null,
    time: null,
    edge: null,
    action: null,
    origin: null,
    hover: { status: "", details: [] },
    faded: null,
    fresh: false,
    open_panel: "Open panel",
    open_panel_keys: "↵",
    open_terminal: "Open terminal",
  },
});

const lane = (
  id: string,
  epic: string | null,
  folded: boolean,
  cells: Record<string, string[]>,
): BoardLane => ({
  id,
  epic,
  head: {
    name: epic === null ? "No epic" : "Wishlist",
    detail: epic === null ? "1 ticket" : "PAY-123 · 5 tickets",
    progress: epic === null ? null : { merged: 1, total: 5, label: "1 of 5 merged" },
    lead: null,
    needs_you: folded,
    pinned: false,
  },
  folded,
  fold: { kind: "fold_lane", epic: id, folded: !folded },
  fold_label: folded ? "Unfold this lane" : "Fold this lane",
  cells: COLUMNS.map((c) => ({ column: c.id, cards: (cells[c.id] ?? []).map(cardOf), folds: [] })),
});

const WISHLIST = lane("SHOP-40", "SHOP-40", false, {
  backlog: ["SHOP-40.3"],
  ready_for_dev: ["SHOP-40.4", "SHOP-40.5"],
  dev: ["SHOP-40.2"],
});
const REST = lane("no-epic", null, true, { dev: ["SHOP-22"] });

const props = (over: Partial<LanesProps> = {}): LanesProps => ({
  columns: COLUMNS,
  lanes: [WISHLIST, REST],
  verdictOf: () => undefined,
  dragging: false,
  selected: null,
  chain: null,
  lifted: null,
  focus: null,
  handlers: {
    press: () => undefined,
    pressChild: () => undefined,
    key: () => undefined,
    open: () => undefined,
    action: () => undefined,
    addToEpic: () => undefined,
    menu: () => undefined,
    hover: () => undefined,
  },
  laneHandlers: { press: () => undefined, menu: () => undefined, open: () => undefined },
  onView: () => undefined,
  onAdd: () => undefined,
  doneLine: { text: "Done 4 →", unfold: { kind: "fold_column", column: "done", folded: false } },
  element: () => undefined,
  ...over,
});

describe("epic lanes (EPL-01, T-0462)", () => {
  it("draws the headers once, then each lane's head and cells; a folded lane is its head with a dot", () => {
    const html = renderToStaticMarkup(createElement(Lanes, props()));
    // The headers are the columns' drop targets; folded Done is its line.
    for (const id of ["backlog", "ready_for_dev", "dev"])
      expect(html).toContain(
        `class="board-lane-header" aria-label="${COLUMNS.find((c) => c.id === id)?.title ?? ""}"`,
      );
    expect(html).toContain('class="board-done-line" data-column="done"');
    expect(html).toContain('data-epic="SHOP-40"');
    expect(html).toContain("PAY-123 · 5 tickets");
    expect(html).toContain('aria-label="1 of 5 merged"');
    expect(html).toContain('style="width:20%"');
    expect(html).toContain('data-card="SHOP-40.4"');
    // "No epic" is folded: its head, no cards, the needs-you dot; it can't be dragged.
    expect(html).toContain('data-lane="no-epic" data-folded="true"');
    expect(html).not.toContain('data-card="SHOP-22"');
    expect(html).toContain("board-lane-dot");
    expect(html).toContain('aria-label="Unfold this lane"');
    expect(html.match(/data-epic=/g)).toHaveLength(1);
  });

  it("dims the other lanes while an epic is focused, and walks the open lanes' cards column by column", () => {
    const html = renderToStaticMarkup(
      createElement(Lanes, props({ focus: { epics: ["SHOP-40"], keys: ["SHOP-40.2"] } })),
    );
    expect(html).toContain(
      'class="board-lane-head" data-epic="SHOP-40" role="button" aria-label="Wishlist" tabindex="0"',
    );
    expect(html).toContain(
      'class="board-lane-head" data-dimmed="true" role="button" aria-label="No epic" tabindex="-1"',
    );
    expect(laneWalk(COLUMNS, [WISHLIST, REST])).toEqual([
      ["SHOP-40.3"],
      ["SHOP-40.4", "SHOP-40.5"],
      ["SHOP-40.2"],
      [],
    ]);
    expect(laneColumns(COLUMNS).map((c) => c.id)).toEqual(["backlog", "ready_for_dev", "dev"]);
  });

  it("outlines a column's header and cells with the drag's verdict", () => {
    const html = renderToStaticMarkup(
      createElement(
        Lanes,
        props({
          dragging: true,
          verdictOf: (id) =>
            id === "ready_for_dev"
              ? { kind: "allowed", icon: "play", hint: "Queue 2 children", note: null }
              : undefined,
        }),
      ),
    );
    expect(html).toContain('data-column="ready_for_dev" data-kind="ready_for_dev" data-verdict="allowed"');
    expect(html).toContain('class="board-lane-cell" data-column="ready_for_dev" data-verdict="allowed"');
  });

  it("a lane-head drag onto Ready for Dev queues the epic's children through board.drop (d0080's path)", async () => {
    const calls: { method: string; params: unknown }[] = [];
    const queue: DragAnswer = {
      key: "SHOP-40",
      targets: [
        { target: "backlog", verdict: { kind: "home" } },
        {
          target: "ready_for_dev",
          verdict: { kind: "allowed", icon: "play", hint: "Queue 2 never-started children", note: null },
        },
      ],
    };
    const ask: Ask = (method, params) => {
      calls.push({ method, params });
      return Promise.resolve(method === "board.drag" ? queue : ({ kind: "moved" } satisfies DropOutcome));
    };
    const drag = createDrag(ask, { onState: () => undefined });
    const spans = () => [
      { id: "backlog", left: 16, right: 196 },
      { id: "ready_for_dev", left: 208, right: 388 },
      { id: "dev", left: 400, right: 580 },
    ];
    // What the lane head's press does (Board's `laneHandlers.press`): the epic's key, as its card's.
    drag.press("SHOP-40", 60, 300);
    drag.move(300, 320, spans);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(liftedLane("SHOP-40", [WISHLIST, REST])?.head.name).toBe("Wishlist");
    expect(liftedLane("no-epic", [WISHLIST, REST])).toBeNull();
    const done = drag.release();
    expect(done).toEqual({ kind: "drop", key: "SHOP-40", target: "ready_for_dev" });
    if (done.kind !== "drop") throw new Error("no drop");
    await dropCard(done.key, done.target, () => undefined, ask);
    expect(calls).toEqual([
      { method: "board.drag", params: { key: "SHOP-40" } },
      { method: "board.drop", params: { key: "SHOP-40", target: "ready_for_dev" } },
    ]);
    // A short press on the head is a click: the board opens the epic.
    drag.press("SHOP-40", 60, 300);
    expect(drag.release()).toEqual({ kind: "click", key: "SHOP-40" });
  });
});
