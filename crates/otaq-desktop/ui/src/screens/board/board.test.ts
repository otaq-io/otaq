import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { cardActions, requestCardAction } from "../../app/cardActions";
import { overlays } from "../../app/overlays";
import { registered, registerScreens } from "../../app/registry";
import type { BoardCard } from "../../generated/BoardCard";
import type { BoardColumn } from "../../generated/BoardColumn";
import type { DragAnswer } from "../../generated/DragAnswer";
import type { DropOutcome } from "../../generated/DropOutcome";
import { afterDrop, coveredWidth, dropCard } from "./Board";
import { EMPTY_BOX, fileAttachment, pastedAttachment, withAdded } from "../editor/commentRules";
import { boxOnScreen, openRequests } from "../new_work/logic";
import { DragBackBody, sendDragBack } from "./DragBackDialog";
import type { CommentBoxView } from "../../generated/CommentBoxView";
import type { DragBackDialog } from "../../generated/DragBackDialog";
import { columnsAfter, moreText, nextScroll } from "./more";
import { FilterStrip } from "./FilterStrip";
import { stepSegment, Toolbar } from "./Toolbar";
import { Column, type ColumnProps, columnOfCard, walkOf } from "./Column";
import {
  aboveChip,
  type Ask,
  columnAt,
  createDrag,
  type DragState,
  fill,
  moveFocus,
  revealScroll,
  targetOrder,
} from "./drag";
import { agentOf, boardAgent, clearAgentFilter, escapeStep, showAgentOnBoard } from "./filter";
import { board } from ".";
import { rememberColumnScroll, scrollPlace } from "./prefs";

afterEach(() => {
  overlays.store.set([]);
  cardActions.set(null);
  boardAgent.set(null);
});

const ANSWER: DragAnswer = {
  key: "SHOP-24",
  targets: [
    { target: "backlog", verdict: { kind: "home" } },
    {
      target: "product-spec",
      verdict: { kind: "allowed", icon: "play", hint: "Drop to start", note: null },
    },
    { target: "dev", verdict: { kind: "allowed", icon: "play", hint: "Drop to start", note: null } },
    {
      target: "review",
      verdict: { kind: "forbidden", title: "Only agents move tickets to Ready for Review", detail: null },
    },
    { target: "approved", verdict: { kind: "forbidden", title: "Approve from Review", detail: null } },
    { target: "done", verdict: { kind: "forbidden", title: "No", detail: null } },
  ],
};

const SPANS = [
  { id: "backlog", left: 10, right: 226 },
  { id: "product-spec", left: 234, right: 450 },
  { id: "dev", left: 458, right: 674 },
  { id: "review", left: 682, right: 1122 },
  { id: "done", left: 1130, right: 1164 },
] as const;

/** A fake door that counts its requests and answers `board.drag` with `ANSWER`. */
function fakeDoor() {
  const calls: { method: string; params: unknown }[] = [];
  let answer: (value: unknown) => void = () => undefined;
  const ask: Ask = (method, params) => {
    calls.push({ method, params });
    return new Promise((resolve) => {
      answer = resolve;
    });
  };
  return {
    calls,
    ask,
    answer: (value: unknown = ANSWER) => {
      answer(value);
    },
  };
}

const cardOf = (key: string): BoardCard => ({
  kind: "ticket",
  needs_you: false,
  card: {
    key,
    priority: null,
    title: key,
    status: null,
    blocks: null,
    avatar: null,
    who: null,
    time: null,
    edge: null,
    action: null,
    note: null,
    opens: null,
    hover: { status: "", details: [] },
    faded: null,
    fresh: false,
    open_sheet: "Open panel",
    open_sheet_keys: "↵",
    open_terminal: "Open terminal",
  },
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("a drag asks Rust once (UIT-04)", () => {
  it("asks at drag start and never per pointer move", async () => {
    const door = fakeDoor();
    const states: (DragState | null)[] = [];
    const drag = createDrag(door.ask, { onState: (s) => states.push(s) });
    let measured = 0;
    const spans = () => {
      measured += 1;
      return SPANS;
    };
    drag.press("SHOP-24", 100, 100);
    expect(drag.move(102, 101, spans)).toBe(false);
    expect(door.calls).toEqual([]);
    for (let x = 105; x < 1100; x += 3) drag.move(x, 120 + (x % 7), spans);
    expect(door.calls).toEqual([{ method: "board.drag", params: { key: "SHOP-24" } }]);
    expect(measured).toBe(1);
    // A state change per column crossed, not per move.
    expect(states.length).toBeLessThan(12);
    door.answer();
    await flush();
    expect(drag.state()?.answer).toEqual(ANSWER);
    expect(door.calls).toHaveLength(1);
  });

  it("drops only on an allowed column, after the answer came", async () => {
    const door = fakeDoor();
    const drag = createDrag(door.ask, { onState: () => undefined });
    drag.press("SHOP-24", 100, 100);
    drag.move(300, 100, () => SPANS);
    expect(drag.release()).toEqual({ kind: "none" });

    drag.press("SHOP-24", 100, 100);
    drag.move(300, 100, () => SPANS);
    door.answer();
    await flush();
    expect(drag.release()).toEqual({ kind: "drop", key: "SHOP-24", target: "product-spec" });

    drag.press("SHOP-24", 120, 100);
    drag.move(700, 100, () => SPANS);
    door.answer();
    await flush();
    const refused = drag.release();
    expect(refused.kind, "a forbidden column refuses, with why").toBe("refused");

    for (const [x, why] of [
      [150, "its own column"],
      [230, "a gap"],
    ] as const) {
      drag.press("SHOP-24", 120, 100);
      drag.move(x, 100, () => SPANS);
      door.answer();
      await flush();
      expect(drag.release(), why).toEqual({ kind: "none" });
    }
  });

  it("a short press is a click, and Esc cancels", async () => {
    const door = fakeDoor();
    const drag = createDrag(door.ask, { onState: () => undefined });
    drag.press("SHOP-24", 100, 100);
    drag.move(101, 101, () => SPANS);
    expect(drag.release()).toEqual({ kind: "click", key: "SHOP-24" });
    drag.press("SHOP-24", 100, 100);
    drag.move(300, 100, () => SPANS);
    door.answer();
    await flush();
    drag.cancel();
    expect(drag.state()).toBeNull();
    expect(drag.release()).toEqual({ kind: "none" });
    expect(door.calls).toHaveLength(1);
  });

  it("an answer for an earlier drag is ignored", async () => {
    const door = fakeDoor();
    const drag = createDrag(door.ask, { onState: () => undefined });
    drag.press("SHOP-24", 100, 100);
    drag.move(300, 100, () => SPANS);
    drag.cancel();
    door.answer();
    await flush();
    expect(drag.state()).toBeNull();
  });

  it("Space lifts, ←/→ move between columns, Space drops", async () => {
    const door = fakeDoor();
    const drag = createDrag(door.ask, { onState: () => undefined });
    const order = SPANS.map((s) => s.id);
    drag.lift("SHOP-24", "backlog");
    door.answer();
    await flush();
    drag.step(-1, order);
    expect(drag.state()?.over).toBe("backlog");
    drag.step(1, order);
    drag.step(1, order);
    expect(drag.state()?.over).toBe("dev");
    // Pointer moves don't move a keyboard drag.
    drag.move(700, 100, () => SPANS);
    expect(drag.state()?.over).toBe("dev");
    expect(drag.release()).toEqual({ kind: "drop", key: "SHOP-24", target: "dev" });
    expect(door.calls).toHaveLength(1);
  });
});

describe("board helpers", () => {
  it("finds the column under the pointer", () => {
    expect(columnAt(SPANS, 300)).toBe("product-spec");
    expect(columnAt(SPANS, 230)).toBeNull();
    expect(columnAt(SPANS, 1140)).toBe("done");
  });

  it("words the scrolled column's chip from Rust's templates", () => {
    const words = { above: "{n} above", need_you: "{n} need you" };
    expect(fill("{n} above", 3)).toBe("3 above");
    expect(aboveChip(words, [])).toBeNull();
    expect(aboveChip(words, [false, false, false])).toEqual({ text: "3 above", needYou: false });
    expect(aboveChip(words, [true, true, false, true, true, true, true])).toEqual({
      text: "7 above · 6 need you",
      needYou: true,
    });
  });

  it("walks cards with the arrow keys, skipping empty columns", () => {
    const columns = [["A", "B"], [], ["C"], ["D", "E", "F"]];
    expect(moveFocus(columns, 0, 0, "ArrowDown")).toEqual({ col: 0, row: 1 });
    expect(moveFocus(columns, 0, 1, "ArrowDown")).toBeNull();
    expect(moveFocus(columns, 0, 1, "ArrowRight")).toEqual({ col: 2, row: 0 });
    expect(moveFocus(columns, 2, 0, "ArrowLeft")).toEqual({ col: 0, row: 0 });
    expect(moveFocus(columns, 3, 2, "ArrowLeft")).toEqual({ col: 2, row: 0 });
    expect(moveFocus(columns, 3, 0, "ArrowRight")).toBeNull();
  });

  it("keeps each project's scroll places (the rest is the service's, SHB-05)", () => {
    rememberColumnScroll("SHOP", "dev", 420);
    expect(scrollPlace("SHOP").columns.dev).toBe(420);
    expect(scrollPlace("DOCS").columns.dev).toBeUndefined();
  });

  it("leaves a card's button press for the sheet", () => {
    requestCardAction("SHOP-25", "resume");
    requestCardAction("SHOP-25", "resume");
    expect(cardActions.get()).toEqual({ ticket: "SHOP-25", kind: "resume", seq: 2 });
  });
});

describe("drops", () => {
  const park: DropOutcome = {
    kind: "park",
    dialog: {
      key: "SHOP-25",
      title: "Park SHOP-25?",
      message: "SHOP-25’s agent will be stopped. Its branch, files and conversation are kept.",
      kept: [{ icon: "git-branch", text: "otaq/SHOP-25" }],
      confirm: "Park",
      confirm_icon: "square-parking",
    },
  };

  it("Park asks first, and its button sends board.park", async () => {
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      return Promise.resolve(null);
    };
    afterDrop(park, ask);
    const open = overlays.store.get();
    expect(open.map((o) => o.id)).toEqual(["confirm"]);
    const confirm = open[0];
    if (confirm?.id !== "confirm") throw new Error("no confirm");
    expect(confirm.params.title).toBe("Park SHOP-25?");
    expect(confirm.params.rows).toEqual([{ icon: "git-branch", text: "otaq/SHOP-25" }]);
    expect(asked).toEqual([]);
    confirm.params.onConfirm();
    await flush();
    expect(asked).toEqual([["board.park", { key: "SHOP-25" }]]);
  });

  it("a move needs nothing more; a not-yet move and a refusal are toasts", async () => {
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      if (method === "board.drop") return Promise.reject(new Error("This move isn't available yet"));
      return Promise.resolve(null);
    };
    afterDrop({ kind: "moved" }, ask);
    expect(asked).toEqual([]);
    afterDrop({ kind: "not_yet", message: "This move isn’t available yet" }, ask);
    expect(asked[0]?.[0]).toBe("notices.toast");
    await dropCard("SHOP-26", "review", ask);
    expect(asked[1]).toEqual(["board.drop", { key: "SHOP-26", target: "review" }]);
    expect(asked[2]).toEqual([
      "notices.toast",
      { level: "error", title: "This move isn't available yet", text: null, link: null, ticket: null },
    ]);
    expect(overlays.store.get()).toEqual([]);
  });
});

describe("drags to any stage (FLW-18, CMT-13)", () => {
  /** A dialog's comment box as `CommentBoxView::dialog` makes it. */
  const commentBox = (label: string, footer: string, placeholder: string): CommentBoxView => ({
    label,
    placeholder,
    submit: null,
    footer,
    footer_icon: "message-square",
    pasted: "Pasted text",
    remove_file: "Remove the file",
    open: "Open",
    remove: "Remove the pasted text",
    attach: "Attach",
    attach_title: "Attach files to the comment",
    optional: true,
    limits: { chars: 2000, report_bytes: 20 * 1024 * 1024, report_label: "20 MB" },
    tell_agent: null,
  });
  const sendBack: DragBackDialog = {
    key: "SHOP-19",
    target: "dev",
    title: "Send SHOP-19 back to Dev?",
    stops: null,
    comment: commentBox(
      "Comment for Dev (Rust developer)",
      "Posted as a comment for Dev · every later stage reads it",
      "What should change? Every later stage reads it too.",
    ),
    then: "Then it goes on to Security, Review and Done as usual.",
    cancel: "Cancel",
    confirm: "Send to Dev",
    confirm_icon: "undo-2",
  };
  const stopAndSend: DragBackDialog = {
    ...sendBack,
    key: "SHOP-29",
    title: "Send SHOP-29 back to Dev?",
    stops: "Security reviewer is working on it now. It stops, and the check runs again after Dev.",
    then: null,
    confirm: "Stop and send to Dev",
  };
  const runAgain: DragBackDialog = {
    key: "SHOP-18",
    target: "security",
    title: "Run Security again on SHOP-18?",
    stops: null,
    comment: commentBox(
      "Comment for Security (Security reviewer)",
      "Posted as a comment for Security · every later stage reads it",
      "",
    ),
    then: null,
    cancel: "Cancel",
    confirm: "Run Security",
    confirm_icon: "rotate-ccw",
  };
  const body = (view: DragBackDialog, refusal: string | null = null) =>
    renderToStaticMarkup(
      createElement(DragBackBody, {
        view,
        comment: EMPTY_BOX,
        onComment: () => undefined,
        onSubmit: () => undefined,
        refusal,
      }),
    );

  it("a drop back to an earlier stage opens the drag-back dialog and sends nothing yet", () => {
    const asked: string[] = [];
    const ask = (method: string) => {
      asked.push(method);
      return Promise.resolve(null);
    };
    afterDrop({ kind: "drag_back", dialog: sendBack }, ask);
    expect(overlays.store.get()).toEqual([{ id: "drag_back", params: sendBack }]);
    expect(asked).toEqual([]);
  });

  it("draws the three dialogs: back to Dev, stop and send, and run a check again", () => {
    const back = body(sendBack);
    expect(back).toContain(">Comment for Dev (Rust developer)</label>");
    expect(back).toContain('placeholder="What should change? Every later stage reads it too."');
    expect(back).toContain("Posted as a comment for Dev · every later stage reads it");
    expect(back).toContain("Then it goes on to Security, Review and Done as usual.");
    expect(back).not.toContain("review-callout");

    const stop = body(stopAndSend);
    expect(stop).toContain('data-tone="paused"');
    expect(stop).toContain(
      "Security reviewer is working on it now. It stops, and the check runs again after Dev.",
    );
    expect(stop).not.toContain("Then it goes on");

    const check = body(runAgain);
    expect(check).toContain(">Comment for Security (Security reviewer)</label>");
    expect(check).toContain("Posted as a comment for Security · every later stage reads it");
    expect(check).toContain('placeholder=""');
    expect(check).not.toContain("Then it goes on");
  });

  it("shows a refusal in the dialog", () => {
    expect(body(sendBack, "SHOP-19 has 2 uncommitted files")).toContain(
      '<p class="review-error" role="alert">SHOP-19 has 2 uncommitted files</p>',
    );
    expect(body(sendBack)).not.toContain("review-error");
  });

  it("its confirm sends board.drag_back; unsent text asks first, then sends again with clear", async () => {
    const asked: [string, unknown][] = [];
    let unsent = true;
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      if (method === "board.drag_back" && unsent) {
        unsent = false;
        return Promise.resolve({
          outcome: "ask",
          title: "SHOP-19's terminal has unsent text",
          body: "…",
          clear: "Clear it and send",
          cancel: "Cancel",
        });
      }
      return Promise.resolve({ outcome: "done" });
    };
    const comment = withAdded({ ...EMPTY_BOX, text: "Keep the old price column" }, [
      pastedAttachment("migration log"),
      fileAttachment({ path: "/tmp/shot.png", name: "shot.png", size: "2 KB", image: true }),
    ]);
    await sendDragBack(sendBack, comment, false, ask);
    expect(asked).toEqual([
      [
        "board.drag_back",
        {
          key: "SHOP-19",
          target: "dev",
          note: "Keep the old price column",
          clear: false,
          attachments: [
            { kind: "pasted", text: "migration log" },
            { kind: "file", path: "/tmp/shot.png" },
          ],
        },
      ],
    ]);
    const confirm = overlays.store.get()[0];
    if (confirm?.id !== "confirm") throw new Error("no unsent-text question");
    expect(confirm.params.confirmLabel).toBe("Clear it and send");
    confirm.params.onConfirm();
    await flush();
    // "Clear it and send" sends the comment and its pasted report again.
    expect(asked[1]).toEqual([
      "board.drag_back",
      {
        key: "SHOP-19",
        target: "dev",
        note: "Keep the old price column",
        clear: true,
        attachments: [
          { kind: "pasted", text: "migration log" },
          { kind: "file", path: "/tmp/shot.png" },
        ],
      },
    ]);
  });
});

describe("the board screen", () => {
  it("fills the board and search slots, the delete dialog; ⌘N opens the box for new work, else the full form", () => {
    const app = registerScreens({ board });
    expect(registered(app).slots).toEqual(["header_search", "board"]);
    expect(registered(app).overlays).toEqual(["delete_ticket", "drag_back"]);
    // The box on screen: it opens, and no form does.
    boxOnScreen.set(true);
    const asked = openRequests.get();
    expect(app.shortcuts.run({ action: "new_ticket" })).toBe(true);
    expect(openRequests.get()).toBe(asked + 1);
    expect(overlays.store.get()).toEqual([]);
    // No box (Noyan's panel open, a full-page ticket): the full form.
    boxOnScreen.set(false);
    expect(app.shortcuts.run({ action: "new_ticket" })).toBe(true);
    expect(overlays.store.get()).toEqual([{ id: "new_ticket", params: undefined }]);
  });

  const column = (over: Partial<BoardColumn>): BoardColumn => ({
    id: "review",
    kind: "human_check",
    title: "Review",
    icon: "eye",
    count: "0",
    quiet_icon: false,
    tooltip: null,
    who_acts: "",
    empty_hint: "Agents move their ticket here when they submit it for your review.",
    collapsed: false,
    rail: null,
    menu: [],
    menu_label: "Column menu",
    hidden: null,
    cards: [],
    dev: null,
    add: null,
    changing: null,
    ...over,
  });

  const props = (c: BoardColumn): ColumnProps => ({
    column: c,
    chipWords: { above: "{n} above", need_you: "{n} need you" },
    dragging: false,
    selected: null,
    lifted: null,
    handlers: {
      press: () => undefined,
      key: () => undefined,
      open: () => undefined,
      action: () => undefined,
      menu: () => undefined,
      hover: () => undefined,
    },
    scrollTop: 0,
    onScroll: () => undefined,
    onView: () => undefined,
    onAdd: () => undefined,
    element: () => undefined,
  });

  const FOLD_MENU = [
    {
      item: {
        id: "fold",
        label: "Fold this column",
        icon: "fold-horizontal" as const,
        shortcut: null,
        danger: false,
        disabled: false,
        hint: null,
        separated: false,
        checked: false,
        header: false,
      },
      change: { kind: "fold_column" as const, column: "review", folded: true },
      editor: null,
    },
  ];

  it("draws an empty column with its hint and its ⋯; folded Done draws nothing of its own", () => {
    const empty = renderToStaticMarkup(createElement(Column, props(column({ menu: FOLD_MENU }))));
    expect(empty).toContain("Agents move their ticket here");
    expect(empty).toContain('data-column="review"');
    expect(empty).toContain('aria-label="Column menu"');
    // Done folded is the board's "Done 4 →" line under the last column (BRD-02).
    const done = renderToStaticMarkup(
      createElement(Column, props(column({ id: "done", title: "Done", count: "2", collapsed: true }))),
    );
    expect(done).toBe("");
    const last = renderToStaticMarkup(
      createElement(Column, {
        ...props(column({})),
        doneLine: { text: "Done 4 →", unfold: { kind: "fold_column", column: "done", folded: false } },
      }),
    );
    expect(last).toContain(
      '<button type="button" class="board-done-line" data-column="done">Done 4 →</button>',
    );
  });

  it("heads a stage with its muted kind icon and who runs it (FLW-12)", () => {
    const dev = renderToStaticMarkup(
      createElement(
        Column,
        props(
          column({
            id: "dev",
            kind: "agent",
            title: "Dev",
            icon: "bot",
            quiet_icon: true,
            count: "3",
            tooltip: { title: "Dev · Rust developer · work-ticket", detail: ["Claude Code · sonnet"] },
          }),
        ),
      ),
    );
    expect(dev).toContain('style="color:var(--text-muted)"');
    expect(dev).toMatch(/board-column-title">Dev<.*board-column-count">3</);
    // The tooltip wraps the title (it shows on hover).
    expect(dev).toMatch(/tooltip-anchor[^>]*><span class="board-column-title">Dev</);
    // The default flow's icons keep their status colour.
    const review = renderToStaticMarkup(createElement(Column, props(column({}))));
    expect(review).toContain('style="color:var(--st-review)"');
  });

  it("draws a folded column as its rail: icon, count, the needs-you dot and its name (`s26-10`)", () => {
    const productSpec = renderToStaticMarkup(
      createElement(
        Column,
        props(
          column({
            id: "product-spec",
            kind: "agent",
            title: "Product spec",
            icon: "bot",
            count: "2",
            collapsed: true,
            rail: {
              tooltip: "Product spec · 2 tickets, 1 needs you",
              hint: "Click to unfold",
              dot: true,
              unfold: { kind: "fold_column", column: "product-spec", folded: false },
            },
          }),
        ),
      ),
    );
    expect(productSpec).toMatch(
      /<button type="button" class="board-rail" data-column="product-spec" aria-label="Product spec · 2 tickets, 1 needs you">/,
    );
    expect(productSpec).toMatch(/board-rail-count">2<.*board-rail-dot.*board-rail-title">Product spec</);
    const calm = renderToStaticMarkup(
      createElement(
        Column,
        props(
          column({
            id: "perf",
            title: "Perf",
            collapsed: true,
            rail: {
              tooltip: "Perf · 0 tickets",
              hint: "Click to unfold",
              dot: false,
              unfold: { kind: "fold_column", column: "perf", folded: false },
            },
          }),
        ),
      ),
    );
    expect(calm).not.toContain("board-rail-dot");
  });

  it("outlines a column with its verdict only while dragging", () => {
    const c = column({ id: "review", title: "Review" });
    const forbidden = { kind: "forbidden", title: "Only agents", detail: null } as const;
    const drawn = renderToStaticMarkup(
      createElement(Column, { ...props(c), verdict: forbidden, dragging: true }),
    );
    expect(drawn).toContain('data-verdict="forbidden"');
    const idle = renderToStaticMarkup(createElement(Column, { ...props(c), verdict: forbidden }));
    expect(idle).not.toContain("data-verdict");
    const home = renderToStaticMarkup(
      createElement(Column, { ...props(c), verdict: { kind: "home" }, dragging: true }),
    );
    expect(home).not.toContain("data-verdict");
  });
});

describe("the Review column (T-0328, d0087)", () => {
  const card = (key: string): BoardCard => ({
    kind: "ticket",
    needs_you: false,
    card: {
      key,
      priority: null,
      title: "Checkout API",
      status: null,
      blocks: null,
      avatar: null,
      who: null,
      time: null,
      edge: null,
      action: null,
      note: null,
      opens: null,
      hover: { status: "Ready for Review", details: [] },
      faded: null,
      fresh: false,
      open_sheet: "Open panel",
      open_sheet_keys: "↵",
      open_terminal: "Open terminal",
    },
  });
  // A ticket waiting for you and one being delivered (PR open): one list, with no sections.
  const review: BoardColumn = {
    id: "review",
    kind: "human_check",
    title: "Review",
    icon: "eye",
    count: "2",
    quiet_icon: false,
    tooltip: null,
    who_acts: "",
    empty_hint: "Agents move their ticket here when they submit it for your review.",
    collapsed: false,
    rail: null,
    menu: [],
    menu_label: "Column menu",
    hidden: null,
    cards: [card("NOTE-7"), card("NOTE-8")],
    dev: null,
    add: null,
    changing: null,
  };
  const props: ColumnProps = {
    column: review,
    chipWords: { above: "{n} above", need_you: "{n} need you" },
    dragging: false,
    selected: null,
    lifted: null,
    handlers: {
      press: () => undefined,
      key: () => undefined,
      open: () => undefined,
      action: () => undefined,
      menu: () => undefined,
      hover: () => undefined,
    },
    scrollTop: 0,
    onScroll: () => undefined,
    onView: () => undefined,
    onAdd: () => undefined,
    element: () => undefined,
  };

  it("lists its cards in one list, with no Approved section; the keyboard walks them all", () => {
    const drawn = renderToStaticMarkup(createElement(Column, props));
    expect(drawn).toContain('data-card="NOTE-7"');
    expect(drawn).toContain('data-card="NOTE-8"');
    expect(drawn).not.toContain("APPROVED");
    expect(drawn).not.toContain("data-section");
    expect(walkOf(review).map((c) => c.card.key)).toEqual(["NOTE-7", "NOTE-8"]);
    expect(targetOrder([{ id: "backlog" }, { id: "dev" }, { id: "review" }, { id: "done" }])).toEqual([
      "backlog",
      "dev",
      "review",
      "done",
    ]);
  });

  it("an empty Review says what comes there", () => {
    const drawn = renderToStaticMarkup(
      createElement(Column, { ...props, column: { ...review, cards: [], count: "0" } }),
    );
    expect(drawn).toContain("Agents move their ticket here when they submit it for your review.");
  });
});

describe("the one width rule's pill (BRD-02)", () => {
  const WORDS = { one: "1 more stage", many: "{n} more stages", at_start: "7 more stages" };
  const spans = [
    { left: 16, right: 196 },
    { left: 208, right: 388 },
    { left: 400, right: 580 },
    { left: 592, right: 772 },
  ];

  it("counts the columns cut off at the right edge, a pixel of rounding aside", () => {
    expect(columnsAfter(spans, 580)).toHaveLength(1);
    expect(columnsAfter(spans, 400)).toHaveLength(2);
    expect(columnsAfter(spans, 771.5)).toHaveLength(0);
    expect(moreText(WORDS, 0)).toBeNull();
    expect(moreText(WORDS, 1)).toBe("1 more stage");
    expect(moreText(WORDS, 7)).toBe("7 more stages");
  });

  it("a click scrolls the first cut-off column to the left edge", () => {
    const after = columnsAfter(spans, 500);
    expect(nextScroll(after, 0, 0, 16)).toBe(384);
    expect(nextScroll(after, 0, 100, 16)).toBe(484);
    expect(nextScroll([], 0, 0, 16)).toBeNull();
  });
});

describe("the toolbar (BRD-01)", () => {
  const option = (label: string, selected: boolean, change: object) => ({
    label,
    icon: null,
    selected,
    change,
    tooltip: null,
    who_acts: "",
    keys: null,
  });
  const TOOLBAR = {
    views: {
      label: null,
      icon: null,
      options: [
        {
          ...option("Board", true, { kind: "view", view: "board" }),
          tooltip: "Board view",
          keys: "g b",
        },
        {
          ...option("Files", false, { kind: "view", view: "files" }),
          tooltip: "Files",
          keys: "⇧⌘O · g f",
        },
      ],
    },
    layouts: {
      label: null,
      icon: null,
      options: [
        {
          ...option("Columns", true, { kind: "view", view: "board" }),
          icon: "columns-3",
          tooltip: "Columns",
          keys: "g b",
        },
        {
          ...option("List", false, { kind: "view", view: "list" }),
          icon: "list",
          tooltip: "List",
          keys: "g l",
        },
      ],
    },
    subtitle: null,
    live: null,
    switch: {
      ...option("Fold empty columns", false, { kind: "fold_empty_columns", on: true }),
      icon: "fold-horizontal",
    },
    all_tickets: "All tickets",
    new_ticket: "New ticket",
    new_ticket_keys: "⌘N",
  } as unknown as Parameters<typeof Toolbar>[0]["toolbar"];

  it("draws Board · Files, the layout switch, the one option, All tickets and New ticket as Rust words them", () => {
    const html = renderToStaticMarkup(
      createElement(Toolbar, {
        toolbar: TOOLBAR,
        onView: () => undefined,
        onAllTickets: () => undefined,
        onNewTicket: () => undefined,
      }),
    );
    expect(html).toMatch(
      /role="radio" class="board-segment" aria-checked="true" tabindex="0">.*<span>Board<\/span>/,
    );
    // The board's only option is Fold empty columns: no Group by, no Dependencies.
    expect(html).not.toContain("Group by");
    expect(html).not.toContain("Dependencies");
    expect(html).toMatch(/role="switch" class="board-chip-button" aria-checked="false">.*Fold empty columns/);
    expect(html).toContain("<span>All tickets</span>");
    expect(html).toContain("<span>New ticket</span>");
    expect(html).toContain("<span>Files</span>");
    // The layout switch: two icons named by their labels, columns chosen.
    expect(html).toMatch(/aria-checked="true" aria-label="Columns"/);
    expect(html).toMatch(/aria-checked="false" aria-label="List"/);
    expect(html).not.toContain("<span>List</span>");
    expect(html).not.toContain("board-toolbar-live");
  });

  it("draws the Files view's subtitle and Live dot, and nothing of the Board's (FIL-01, s28-03)", () => {
    const views = TOOLBAR.views.options.map((o) => ({ ...o, selected: o.label === "Files" }));
    const files = {
      ...TOOLBAR,
      views: { ...TOOLBAR.views, options: views },
      subtitle: "What the base branch looks like, and which agents are changing which files",
      live: "Live",
      layouts: null,
      switch: null,
      all_tickets: null,
      new_ticket: null,
      new_ticket_keys: null,
    } as unknown as Parameters<typeof Toolbar>[0]["toolbar"];
    const html = renderToStaticMarkup(
      createElement(Toolbar, {
        toolbar: files,
        onView: () => undefined,
        onAllTickets: () => undefined,
        onNewTicket: () => undefined,
      }),
    );
    expect(html).toContain(
      '<span class="board-toolbar-subtitle">What the base branch looks like, and which agents are changing which files</span>',
    );
    expect(html).toMatch(/board-toolbar-live-dot.*<span>Live<\/span>/);
    expect(html).toMatch(/aria-checked="true" tabindex="0">.*<span>Files<\/span>/);
    expect(html).not.toContain("All tickets");
    expect(html).not.toContain('role="switch"');
    expect(html).not.toContain('aria-label="Layout"');
  });

  it("←/→ step through a segment group, wrapping", () => {
    expect(stepSegment("ArrowRight", 0, 2)).toBe(1);
    expect(stepSegment("ArrowRight", 1, 2)).toBe(0);
    expect(stepSegment("ArrowLeft", 0, 4)).toBe(3);
    expect(stepSegment("Enter", 0, 2)).toBeNull();
  });
});

describe("the sheet over the board (SHL-05)", () => {
  it("scrolls to the selected card's column", () => {
    const columns: BoardColumn[] = (["backlog", "dev", "review"] as const).map((id) => ({
      id,
      kind: id === "dev" ? "agent" : id === "review" ? "human_check" : "backlog",
      title: id,
      icon: "eye",
      count: "0",
      quiet_icon: false,
      tooltip: null,
      who_acts: "",
      hint: null,
      empty_hint: "",
      collapsed: false,
      rail: null,
      menu: [],
      menu_label: "Column menu",
      hidden: null,
      cards: id === "dev" ? [{ ...cardOf("NOTE-6") }] : id === "review" ? [{ ...cardOf("NOTE-7") }] : [],
      dev: null,
      add: null,
      changing: null,
    }));
    expect(columnOfCard(columns, "NOTE-6")).toBe("dev");
    expect(columnOfCard(columns, "NOTE-7")).toBe("review");
    expect(columnOfCard(columns, "NOTE-10.1")).toBeNull();
    // x9qiI at 1280: the board shows 10…306 left of the 600 px sheet; Dev (463…687 in the board) goes to its left.
    const view = { left: 394, right: 680 };
    expect(revealScroll({ left: 857, right: 1081 }, view, 0)).toBe(463);
    expect(revealScroll({ left: 400, right: 620 }, view, 120), "already in view").toBeNull();
    expect(revealScroll({ left: 100, right: 324 }, view, 300), "scrolled past, back").toBe(6);
    expect(coveredWidth({ right: 1280 }, { left: 680 })).toBe(600);
    expect(coveredWidth({ right: 1280 }, null)).toBe(0);
  });
});

describe("the agent filter (d0087)", () => {
  it("is kept for its project, and ✕ clears it", () => {
    showAgentOnBoard("NOTE", "a1");
    expect(agentOf(boardAgent.get(), "NOTE")).toBe("a1");
    expect(agentOf(boardAgent.get(), "API")).toBeNull();
    clearAgentFilter("API");
    expect(agentOf(boardAgent.get(), "NOTE")).toBe("a1");
    clearAgentFilter("NOTE");
    expect(boardAgent.get()).toBeNull();
  });

  it("Esc undoes a drag, then the agent filter", () => {
    expect(escapeStep(true, true)).toBe("drag");
    expect(escapeStep(false, true)).toBe("agent");
    expect(escapeStep(false, false)).toBeNull();
  });
});

describe("the filter strip", () => {
  it("says how many show, the chips, one filter to add and Clear, as Rust words them", () => {
    const html = renderToStaticMarkup(
      createElement(FilterStrip, {
        project: "SHOP",
        strip: {
          showing: "Showing 3 of 17",
          chips: [{ label: "“checkout”", token: null }],
          suggest: "is:needs-you",
          suggest_label: "+ is:needs-you",
          clear: "Clear",
        },
      }),
    );
    expect(html).toContain("Showing 3 of 17");
    expect(html).toContain("“checkout”");
    expect(html).toContain("+ is:needs-you");
    expect(html).toContain(">Clear</button>");
  });
});
