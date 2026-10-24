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
import type { BoardReviewSection } from "../../generated/BoardReviewSection";
import { afterDrop, coveredWidth, dropCard, openApproveChoice } from "./Board";
import { EMPTY_BOX } from "../editor/commentRules";
import { DragBackBody, sendDragBack } from "./DragBackDialog";
import { BlockedBody } from "./BlockedDialog";
import type { BlockedDialog } from "../../generated/BlockedDialog";
import type { CommentBoxView } from "../../generated/CommentBoxView";
import type { DragBackDialog } from "../../generated/DragBackDialog";
import { columnsAfter, moreText, nextScroll } from "./more";
import { stepSegment, Toolbar } from "./Toolbar";
import { Column, type ColumnProps, columnOfCard, runsOf, walkOf } from "./Column";
import {
  aboveChip,
  type Ask,
  columnAt,
  createDrag,
  type DragState,
  fill,
  liftedRow,
  moveFocus,
  revealScroll,
  targetOrder,
} from "./drag";
import { agentOf, boardAgent, clearAgentFilter, escapeStep, showAgentOnBoard } from "./filter";
import type { EpicCardView } from "../../generated/EpicCardView";
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
      target: "ready_for_dev",
      verdict: { kind: "allowed", icon: "play", hint: "Drop to queue", note: null },
    },
    { target: "dev", verdict: { kind: "allowed", icon: "play", hint: "Drop to queue", note: null } },
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
  { id: "ready_for_dev", left: 234, right: 450 },
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
    key_label: key,
    epic: null,
    priority: null,
    title: key,
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

const sectionOf = (key: string): BoardReviewSection => ({
  id: "ready_for_review",
  target: null,
  title: "READY FOR REVIEW",
  count: "1",
  cards: [cardOf(key)],
  hidden: null,
  empty: "",
  note: null,
  drag_hint: null,
  fold: "",
  unfold: "",
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
    expect(drag.release()).toEqual({ kind: "drop", key: "SHOP-24", target: "ready_for_dev" });

    for (const [x, why] of [
      [700, "forbidden"],
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
    expect(columnAt(SPANS, 300)).toBe("ready_for_dev");
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

  it("leaves a card's button press for the panel", () => {
    requestCardAction("SHOP-25", "resume");
    requestCardAction("SHOP-25", "resume");
    expect(cardActions.get()).toEqual({ ticket: "SHOP-25", kind: "resume", seq: 2 });
  });
});

describe("dragging an epic's child out (T-0270)", () => {
  const row = (key: string, draggable: boolean) => ({
    key,
    title: `${key} title`,
    state: "not_started" as const,
    icon: "circle-dashed" as const,
    status_label: "Backlog",
    draggable,
  });
  const epic = { children: [row("SHOP-45.1", true), row("SHOP-45.2", false)] } as EpicCardView;

  it("lifts a draggable row with its title, and nothing else", () => {
    expect(liftedRow("SHOP-45.1", [epic])).toEqual({ key: "SHOP-45.1", title: "SHOP-45.1 title" });
    expect(liftedRow("SHOP-45.2", [epic]), "not draggable").toBeNull();
    expect(liftedRow("SHOP-24", [epic]), "a card draws itself").toBeNull();
  });

  it("a row dropped on Ready for Dev queues only that child", async () => {
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      return Promise.resolve({ kind: "moved" } satisfies DropOutcome);
    };
    await dropCard("SHOP-40.3", "ready_for_dev", () => undefined, ask);
    expect(asked).toEqual([["board.drop", { key: "SHOP-40.3", target: "ready_for_dev" }]]);
  });
});

describe("an epic taken back (T-0271)", () => {
  it("a press that isn't a click (`clicks` false) is no click; dragged, it drops like a card", async () => {
    const door = fakeDoor();
    const drag = createDrag(door.ask, { onState: () => undefined });
    drag.press("SHOP-45", 100, 100, false);
    expect(drag.release()).toEqual({ kind: "none" });
    expect(door.calls).toEqual([]);
    // Dragged, it drops like a card.
    drag.press("SHOP-45", 300, 100, false);
    drag.move(150, 100, () => SPANS);
    door.answer({
      key: "SHOP-45",
      targets: [
        {
          target: "backlog",
          verdict: {
            kind: "allowed",
            icon: "arrow-down-to-line",
            hint: "Unqueue 1 never-started child (45.2)",
            note: "SHOP-45.1 keeps running",
          },
        },
      ],
    } satisfies DragAnswer);
    await flush();
    expect(drag.release()).toEqual({ kind: "drop", key: "SHOP-45", target: "backlog" });
  });

  it("Park all asks first with Cancel as the default, and its button sends board.park_all", async () => {
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      return Promise.resolve(null);
    };
    const outcome: DropOutcome = {
      kind: "park_all",
      dialog: {
        key: "SHOP-45",
        title: "Park 2 running tickets?",
        message:
          "Their work is kept. SHOP-45.1 and SHOP-45.2 stop and go back to Backlog with their conversations.",
        cancel: "Cancel",
        confirm: "Park 2 tickets",
        confirm_icon: "square-parking",
      },
    };
    afterDrop(outcome, () => undefined, ask);
    const open = overlays.store.get();
    const confirm = open[0];
    if (confirm?.id !== "confirm") throw new Error("no confirm");
    expect(confirm.params.title).toBe("Park 2 running tickets?");
    expect(confirm.params.confirmLabel).toBe("Park 2 tickets");
    expect(confirm.params.cancelDefault).toBe(true);
    expect(confirm.params.destructive).toBe(false);
    expect(asked).toEqual([]);
    confirm.params.onConfirm();
    await flush();
    expect(asked).toEqual([["board.park_all", { key: "SHOP-45" }]]);
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
    afterDrop(park, () => undefined, ask);
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

  it("a drop on Approved shows the approve choice and moves nothing; its choices open their dialogs", async () => {
    const menus: string[] = [];
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      return Promise.resolve(
        method === "board.drop"
          ? ({
              kind: "approve",
              menu: { key: "SHOP-18", title: "Approve SHOP-18 Checkout API", items: [], note: "" },
            } satisfies DropOutcome)
          : null,
      );
    };
    await dropCard("SHOP-18", "approved", (menu) => menus.push(menu.title), ask);
    expect(menus).toEqual(["Approve SHOP-18 Checkout API"]);
    // The drop only asked what to do: until 1 or 2 is picked nothing else is sent, so Esc leaves the card in Ready
    // for Review (the choice closes without a request).
    expect(asked).toEqual([["board.drop", { key: "SHOP-18", target: "approved" }]]);
    expect(overlays.store.get()).toEqual([]);
    openApproveChoice("local_merge", "SHOP-18");
    openApproveChoice("push_to_pr", "SHOP-26");
    expect(overlays.store.get()).toEqual([
      { id: "local_merge", params: { ticket: "SHOP-18" } },
      { id: "open_pr", params: { ticket: "SHOP-26" } },
    ]);
  });

  it("a move needs nothing more; a not-yet move and a refusal are toasts", async () => {
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      if (method === "board.drop") return Promise.reject(new Error("PR rework isn't available yet"));
      return Promise.resolve(null);
    };
    afterDrop({ kind: "moved" }, () => undefined, ask);
    expect(asked).toEqual([]);
    afterDrop({ kind: "not_yet", message: "This move isn’t available yet" }, () => undefined, ask);
    expect(asked[0]?.[0]).toBe("notices.toast");
    await dropCard("SHOP-26", "review", () => undefined, ask);
    expect(asked[1]).toEqual(["board.drop", { key: "SHOP-26", target: "review" }]);
    expect(asked[2]).toEqual([
      "notices.toast",
      { level: "error", title: "PR rework isn't available yet", text: null, link: null, ticket: null },
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
    needs_line: "Say what the next agent should look for",
    pasted: "Pasted text",
    file_line: "A comment always has its own line; the file goes with it.",
    remove_file: "Remove the file",
    open: "Open",
    remove: "Remove the pasted text",
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
    afterDrop({ kind: "drag_back", dialog: sendBack }, () => undefined, ask);
    expect(overlays.store.get()).toEqual([{ id: "drag_back", params: sendBack }]);
    expect(asked).toEqual([]);
  });

  it("a blocked ticket's drop opens DPV-03's dialog, whose only button keeps it waiting", () => {
    const blocked: BlockedDialog = {
      key: "SHOP-20",
      title: "SHOP-20 is blocked",
      body: "It waits for SHOP-18 (in Review) and SHOP-29 (in Dev) to merge. Started now, its agent works without their changes and may have to redo work when they land.",
      hint: "To start it, remove the dependency in the ticket first.",
      keep: "Keep it waiting",
    };
    const asked: string[] = [];
    afterDrop(
      { kind: "blocked", dialog: blocked },
      () => undefined,
      (method: string) => {
        asked.push(method);
        return Promise.resolve(null);
      },
    );
    expect(overlays.store.get()).toEqual([{ id: "blocked", params: blocked }]);
    expect(asked).toEqual([]);
    const out = renderToStaticMarkup(createElement(BlockedBody, { view: blocked }));
    expect(out).toContain("It waits for SHOP-18 (in Review) and SHOP-29 (in Dev) to merge.");
    expect(out).toContain("To start it, remove the dependency in the ticket first.");
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
    const comment = {
      text: "Keep the old price column",
      pasted: { text: "migration log", bytes: 13 },
      file: null,
    };
    await sendDragBack(sendBack, comment, false, ask);
    expect(asked).toEqual([
      [
        "board.drag_back",
        {
          key: "SHOP-19",
          target: "dev",
          note: "Keep the old price column",
          clear: false,
          report: "migration log",
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
        report: "migration log",
      },
    ]);
  });
});

describe("the board screen", () => {
  it("fills the board and search slots, the delete dialog, and ⌘N opens a new ticket", () => {
    const app = registerScreens({ board });
    expect(registered(app).slots).toEqual(["header_search", "board"]);
    expect(registered(app).overlays).toEqual(["delete_ticket", "drag_back", "blocked"]);
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
    folded: [],
    tooltip: null,
    hint: null,
    empty_hint: "Agents move their ticket here when they submit it for your review.",
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

  const props = (c: BoardColumn): ColumnProps => ({
    column: c,
    chipWords: { above: "{n} above", need_you: "{n} need you" },
    dragging: false,
    selected: null,
    lifted: null,
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

  it("heads a stage with its muted kind icon, its folded stages and who runs it (FLW-12)", () => {
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
            folded: [{ tag: "Validating", count: "1" }],
            tooltip: { title: "Dev · Rust developer · work-ticket", detail: ["Claude Code · sonnet"] },
          }),
        ),
      ),
    );
    expect(dev).toContain('style="color:var(--text-muted)"');
    expect(dev).toMatch(/board-column-title">Dev<.*board-column-count">3<.*·.*Validating<.*>1</);
    // The tooltip wraps the title (it shows on hover).
    expect(dev).toMatch(/tooltip-anchor[^>]*><span class="board-column-title">Dev</);
    // The default flow's icons keep their status colour.
    const review = renderToStaticMarkup(createElement(Column, props(column({}))));
    expect(review).toContain('style="color:var(--st-review)"');
    expect(review).not.toContain("board-column-folded");
  });

  it("draws a folded column as its rail: icon, count, the needs-you dot and its name (`s26-10`)", () => {
    const grill = renderToStaticMarkup(
      createElement(
        Column,
        props(
          column({
            id: "grill",
            kind: "agent",
            title: "Grill",
            icon: "bot",
            count: "2",
            collapsed: true,
            rail: {
              tooltip: "Grill · 2 tickets, 1 needs you",
              hint: "Click to unfold",
              dot: true,
              unfold: { kind: "fold_column", column: "grill", folded: false },
            },
          }),
        ),
      ),
    );
    expect(grill).toMatch(
      /<button type="button" class="board-rail" data-column="grill" aria-label="Grill · 2 tickets, 1 needs you">/,
    );
    expect(grill).toMatch(/board-rail-count">2<.*board-rail-dot.*board-rail-title">Grill</);
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

  const ticketCard = (key: string): BoardCard => ({
    kind: "ticket",
    needs_you: false,
    card: {
      key,
      key_label: key,
      epic: null,
      priority: null,
      title: "Child",
      status: null,
      blocks: null,
      avatar: null,
      who: null,
      time: null,
      edge: null,
      action: null,
      origin: null,
      hover: { status: "Merged", details: [] },
      faded: null,
      fresh: false,
      open_panel: "Open panel",
      open_panel_keys: "↵",
      open_terminal: "Open terminal",
    },
  });

  it("folds an epic's merged children in Done into one line; the keyboard skips them", () => {
    const fold = { epic: "SHOP-40", label: "Wishlist · 2 of 4 merged", keys: ["SHOP-40.1", "SHOP-40.2"] };
    const cards = ["SHOP-17", "SHOP-40.1", "SHOP-40.2", "SHOP-40.5"].map(ticketCard);
    const done = column({ id: "done", title: "Done", count: "3", cards, folds: [fold] });
    expect(runsOf(cards, [fold]).map((r) => [r.kind, r.cards.map((c) => c.card.key)])).toEqual([
      ["cards", ["SHOP-17"]],
      ["fold", ["SHOP-40.1", "SHOP-40.2"]],
      ["cards", ["SHOP-40.5"]],
    ]);
    expect(walkOf(done).map((c) => c.card.key)).toEqual(["SHOP-17", "SHOP-40.5"]);
    const drawn = renderToStaticMarkup(createElement(Column, props(done)));
    expect(drawn).toContain("Wishlist · 2 of 4 merged");
    expect(drawn).toContain('aria-expanded="false"');
    expect(drawn).toContain('data-card="SHOP-17"');
    expect(drawn).toContain('data-card="SHOP-40.5"');
    expect(drawn).not.toContain('data-card="SHOP-40.1"');
    expect(drawn.indexOf("SHOP-17")).toBeLessThan(drawn.indexOf("Wishlist · 2 of 4"));
    expect(drawn.indexOf("Wishlist · 2 of 4")).toBeLessThan(drawn.indexOf("SHOP-40.5"));
  });

  it("dims the cards and Done lines of other epics while one is focused (EPC-18, Q55)", () => {
    const wishlist = { epic: "SHOP-40", label: "Wishlist · 1 of 3 merged", keys: ["SHOP-40.1"] };
    const search = { epic: "SHOP-45", label: "Search · 1 of 2 merged", keys: ["SHOP-45.1"] };
    const cards = ["SHOP-17", "SHOP-40.1", "SHOP-45.1"].map(ticketCard);
    const done = column({ id: "done", title: "Done", count: "1/3", cards, folds: [wishlist, search] });
    const focus = { epics: ["SHOP-40"], keys: ["SHOP-40", "SHOP-40.1", "SHOP-40.2"] };
    const drawn = renderToStaticMarkup(createElement(Column, { ...props(done), focus }));
    expect(drawn).toMatch(/data-card="SHOP-17"[^>]*data-dimmed="true"/);
    expect(drawn).toMatch(/data-epic="SHOP-45"[^>]*data-dimmed="true"/);
    expect(drawn).not.toMatch(/data-epic="SHOP-40"[^>]*data-dimmed/);
    const open = renderToStaticMarkup(createElement(Column, props(done)));
    expect(open).not.toContain("data-dimmed");
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
  // SHOP-18 is Ready for Review: Review is its home, Approved asks the Approve choice.
  const RFR: DragAnswer = {
    key: "SHOP-18",
    targets: [
      { target: "backlog", verdict: { kind: "forbidden", title: "No", detail: null } },
      { target: "ready_for_dev", verdict: { kind: "forbidden", title: "No", detail: null } },
      { target: "dev", verdict: { kind: "forbidden", title: "No", detail: null } },
      { target: "review", verdict: { kind: "home" } },
      {
        target: "approved",
        verdict: {
          kind: "allowed",
          icon: "circle-check",
          hint: "Drop to approve SHOP-18 → choose Local merge or Open PR",
          note: null,
        },
      },
      { target: "done", verdict: { kind: "forbidden", title: "No", detail: null } },
    ],
  };
  // Approved's section: from its label (y 300) down to the column's bottom.
  const REVIEW_SPANS = [...SPANS, { id: "approved", left: 682, right: 1122, top: 300, bottom: 900 }] as const;

  it("finds Approved by its section and the rest of Review by its column", () => {
    expect(columnAt(REVIEW_SPANS, 700, 320)).toBe("approved");
    expect(columnAt(REVIEW_SPANS, 700, 200)).toBe("review");
    expect(columnAt(REVIEW_SPANS, 300, 320)).toBe("ready_for_dev");
    const sections = (id: string) =>
      id === "review"
        ? [{ ...sectionOf("NOTE-7") }, { ...sectionOf("NOTE-8"), id: "approved", target: "approved" }]
        : [];
    expect(
      targetOrder(
        ["backlog", "ready_for_dev", "dev", "review", "done"].map((id) => ({ id, sections: sections(id) })),
      ),
    ).toEqual(["backlog", "ready_for_dev", "dev", "review", "approved", "done"]);
  });

  it("looks up each section's verdict: Ready for Review drops on Approved, not on its own section", async () => {
    let answer: (value: unknown) => void = () => undefined;
    const drag = createDrag(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
      { onState: () => undefined },
    );
    drag.press("SHOP-18", 700, 150);
    drag.move(705, 160, () => REVIEW_SPANS);
    answer(RFR);
    await flush();
    expect(drag.state()?.over).toBe("review");
    drag.move(705, 400, () => REVIEW_SPANS);
    expect(drag.state()?.over).toBe("approved");
    expect(drag.release()).toEqual({ kind: "drop", key: "SHOP-18", target: "approved" });

    drag.press("SHOP-18", 700, 150);
    drag.move(705, 200, () => REVIEW_SPANS);
    answer(RFR);
    await flush();
    expect(drag.release(), "home").toEqual({ kind: "none" });
  });

  const card = (key: string): BoardCard => ({
    kind: "ticket",
    needs_you: false,
    card: {
      key,
      key_label: key,
      epic: null,
      priority: null,
      title: "Checkout API",
      status: null,
      blocks: null,
      avatar: null,
      who: null,
      time: null,
      edge: null,
      action: null,
      origin: null,
      hover: { status: "Ready for Review", details: [] },
      faded: null,
      fresh: false,
      open_panel: "Open panel",
      open_panel_keys: "↵",
      open_terminal: "Open terminal",
    },
  });
  const section = (over: Partial<BoardReviewSection>): BoardReviewSection => ({
    id: "ready_for_review",
    target: null,
    title: "READY FOR REVIEW",
    count: "0",
    cards: [],
    hidden: null,
    empty: "Nothing to review",
    note: null,
    drag_hint: null,
    fold: "Fold Ready for Review",
    unfold: "Unfold Ready for Review",
    ...over,
  });
  const review: BoardColumn = {
    id: "review",
    kind: "human_check",
    title: "Review",
    icon: "eye",
    count: "1",
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
    sections: [
      section({ count: "1", cards: [card("NOTE-7")] }),
      section({
        id: "approved",
        target: "approved",
        title: "APPROVED",
        empty: "Nothing approved",
        drag_hint: "Drag a ticket here from Ready for Review to approve it.",
        fold: "Fold Approved",
        unfold: "Unfold Approved",
      }),
    ],
    folds: [],
    dev: null,
    add: null,
  };
  const props: ColumnProps = {
    column: review,
    chipWords: { above: "{n} above", need_you: "{n} need you" },
    dragging: false,
    selected: null,
    lifted: null,
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
    scrollTop: 0,
    onScroll: () => undefined,
    onView: () => undefined,
    onAdd: () => undefined,
    element: () => undefined,
  };

  it("draws both sections with their counts, and an empty one's line", () => {
    const drawn = renderToStaticMarkup(createElement(Column, props));
    expect(drawn).toContain("READY FOR REVIEW");
    expect(drawn).toContain('data-card="NOTE-7"');
    expect(drawn).toContain("Nothing approved");
    expect(drawn.indexOf("NOTE-7")).toBeLessThan(drawn.indexOf("APPROVED"));
  });

  it("a folded section keeps its label and count, hides its cards, and the keyboard skips them", () => {
    const drawn = renderToStaticMarkup(
      createElement(Column, { ...props, foldedSections: ["ready_for_review"] }),
    );
    expect(drawn).toContain("READY FOR REVIEW");
    expect(drawn).toMatch(/aria-expanded="false"[^]*READY FOR REVIEW[^]*>1</);
    expect(drawn).not.toContain('data-card="NOTE-7"');
    expect(walkOf(review, ["ready_for_review"])).toEqual([]);
    expect(walkOf(review).map((c) => c.card.key)).toEqual(["NOTE-7"]);
  });

  it("lights Approved with the hint only while the card is over it; an empty Approved says how meanwhile", () => {
    const verdict = RFR.targets.find((t) => t.target === "approved")?.verdict;
    const near = renderToStaticMarkup(
      createElement(Column, { ...props, dragging: true, approved: { verdict, over: false } }),
    );
    expect(near).toContain("Drag a ticket here from Ready for Review");
    expect(near).not.toContain("Drop to approve");
    expect(near).not.toContain('data-section="approved" data-verdict');
    const over = renderToStaticMarkup(
      createElement(Column, { ...props, dragging: true, approved: { verdict, over: true } }),
    );
    expect(over).toContain('data-section="approved" data-verdict="allowed"');
    expect(over).toContain("Drop to approve SHOP-18 → choose Local merge or Open PR");
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
    keys: null,
  });
  const TOOLBAR = {
    views: {
      label: null,
      icon: null,
      options: [
        {
          ...option("Board", true, { kind: "view", view: "board" }),
          icon: "columns-3",
          tooltip: "Board view",
          keys: "g b",
        },
        {
          ...option("List", false, { kind: "view", view: "list" }),
          icon: "list",
          tooltip: "List view",
          keys: "g l",
        },
        {
          ...option("Files", false, { kind: "view", view: "files" }),
          icon: "folder-open",
          tooltip: "Files",
          keys: "⇧⌘O · g f",
        },
      ],
    },
    subtitle: null,
    live: null,
    lanes: {
      label: "Lanes",
      icon: null,
      options: [
        option("None", true, { kind: "lanes", lanes: "none" }),
        option("Epic", false, { kind: "lanes", lanes: "epic" }),
      ],
    },
    group_by: null,
    dependencies: {
      label: "Dependencies",
      icon: "link",
      options: [
        option("On hover", true, { kind: "dependencies", dependencies: "on_hover" }),
        option("Always", false, { kind: "dependencies", dependencies: "always" }),
      ],
    },
    switch: {
      ...option("Fold empty columns", false, { kind: "fold_empty_columns", on: true }),
      icon: "fold-horizontal",
    },
    all_tickets: "All tickets",
    new_ticket: "New ticket",
    new_ticket_keys: "⌘N",
  } as unknown as Parameters<typeof Toolbar>[0]["toolbar"];

  it("draws Board · List, Lanes, the switch, All tickets and New ticket as Rust words them", () => {
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
    expect(html).toContain('<span class="board-toolbar-label">Lanes</span>');
    // DPV-02: the link, "Dependencies", then On hover · Always.
    expect(html).toMatch(
      /board-toolbar-label-icon.*<span class="board-toolbar-label">Dependencies<\/span><div class="board-segments" role="radiogroup" aria-label="Dependencies">.*<span>On hover<\/span>.*<span>Always<\/span>/,
    );
    expect(html).toMatch(/role="switch" class="board-chip-button" aria-checked="false">.*Fold empty columns/);
    expect(html).toContain("<span>All tickets</span>");
    expect(html).toContain("<span>New ticket</span>");
    expect(html).toContain("<span>Files</span>");
    expect(html).not.toContain("board-toolbar-live");
  });

  it("draws the Files view's subtitle and Live dot, and nothing of the Board's (FIL-01, s28-03)", () => {
    const views = TOOLBAR.views.options.map((o) => ({ ...o, selected: o.label === "Files" }));
    const files = {
      ...TOOLBAR,
      views: { ...TOOLBAR.views, options: views },
      subtitle: "What the base branch looks like, and which agents are changing which files",
      live: "Live",
      lanes: null,
      dependencies: null,
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
  });

  it("←/→ step through a segment group, wrapping", () => {
    expect(stepSegment("ArrowRight", 0, 2)).toBe(1);
    expect(stepSegment("ArrowRight", 1, 2)).toBe(0);
    expect(stepSegment("ArrowLeft", 0, 4)).toBe(3);
    expect(stepSegment("Enter", 0, 2)).toBeNull();
  });
});

describe("the panel over the board (SHL-05)", () => {
  it("scrolls to the selected card's column", () => {
    const columns: BoardColumn[] = (["backlog", "dev", "review"] as const).map((id) => ({
      id,
      kind: id === "dev" ? "agent" : id === "review" ? "human_check" : "backlog",
      title: id,
      icon: "eye",
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
      cards: id === "dev" ? [{ ...cardOf("NOTE-6") }] : [],
      sections: id === "review" ? [{ ...sectionOf("NOTE-7") }] : [],
      folds: [],
      dev: null,
      add: null,
    }));
    expect(columnOfCard(columns, "NOTE-6")).toBe("dev");
    expect(columnOfCard(columns, "NOTE-7")).toBe("review");
    expect(columnOfCard(columns, "NOTE-10.1")).toBeNull();
    // x9qiI at 1280: the board shows 10…306 left of the 600 px panel; Dev (463…687 in the board) goes to its left.
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

  it("Esc undoes a drag, then an epic focus, then the agent filter", () => {
    expect(escapeStep(true, true, true)).toBe("drag");
    expect(escapeStep(false, true, true)).toBe("focus");
    expect(escapeStep(false, false, true)).toBe("agent");
    expect(escapeStep(false, false, false)).toBeNull();
  });
});
