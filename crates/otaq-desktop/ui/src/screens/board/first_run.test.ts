import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { createStore } from "../../app/store";
import type { BoardHint } from "../../generated/BoardHint";
import type { FirstTicketView } from "../../generated/FirstTicketView";
import type { NotificationsLine } from "../../generated/NotificationsLine";
import { openTicket, selection } from "../../app/selection";
import { flowEditing } from "../flow_board/logic";
import { answerFirstTicket, HINT_ROUTES, hintTarget, pressHint, startFirstTicket } from "./first_run";
import { FirstTicketCard } from "./FirstRun";

const LINE: NotificationsLine = {
  title: "Turn on notifications",
  text: "So you know when an agent asks you something, even with Otaq closed.",
  button: "OK",
};

/** A fake door that records each request and answers from `answers`. */
function door(answers: Record<string, unknown>) {
  const asked: { method: string; params: unknown }[] = [];
  const ask = (method: string, params?: unknown) => {
    asked.push({ method, params });
    return Promise.resolve(answers[method] ?? null);
  };
  return { asked, ask };
}

describe("the first ticket", () => {
  it("Create & start asks for notifications first, then creates and starts it, and answers the card", async () => {
    const { asked, ask } = door({
      "notices.first_start": LINE,
      "editor.create_and_start": { outcome: "saved", key: "SHOP-1", note: null },
    });
    const line = createStore<NotificationsLine | null>(null);
    const saved = await startFirstTicket("SHOP", " Tidy the README ", "Fix typos.", ask, line);
    expect(saved.outcome).toBe("saved");
    expect(asked.map((a) => a.method)).toEqual([
      "notices.first_start",
      "editor.create_and_start",
      "hints.dismiss",
    ]);
    expect(asked[1]?.params).toMatchObject({
      target: { mode: "create", project: "SHOP" },
      form: { title: "Tidy the README", body: "Fix typos." },
    });
    expect(asked[2]?.params).toEqual({ hint: "first_ticket" });
    expect(line.get()).toEqual(LINE);
  });

  it("asks nothing more once macOS was asked, and keeps the card when nothing was created", async () => {
    const { asked, ask } = door({
      "notices.first_start": null,
      "editor.create_and_start": { outcome: "title_missing" },
    });
    const line = createStore<NotificationsLine | null>(null);
    await startFirstTicket("SHOP", "", "", ask, line);
    expect(line.get()).toBeNull();
    expect(asked.map((a) => a.method)).toEqual(["notices.first_start", "editor.create_and_start"]);
  });

  it("Not now answers the card for good", async () => {
    const { asked, ask } = door({});
    await answerFirstTicket(ask);
    expect(asked).toEqual([{ method: "hints.dismiss", params: { hint: "first_ticket" } }]);
  });

  it("draws the prefilled card with the tip, the cost line and both buttons", () => {
    const view: FirstTicketView = {
      label: "Your first ticket",
      status: "Not started",
      title: "Tidy the README",
      title_label: "Title",
      body: "Fix typos and broken links. Change nothing else.",
      body_label: "Description",
      tip: "Small, clear tasks work best.",
      cost: "Takes about 5 minutes and uses your Claude Code plan.",
      start: "Create & start",
      starting: "Starting…",
      not_now: "Not now",
      no_title: "Give it a title first.",
    };
    const html = renderToStaticMarkup(
      createElement(FirstTicketCard, { view, project: "SHOP", onAnswered: () => undefined }),
    );
    for (const text of [
      "Your first ticket",
      'value="Tidy the README"',
      "Fix typos and broken links. Change nothing else.",
      "Small, clear tasks work best.",
      "Takes about 5 minutes and uses your Claude Code plan.",
      "Create &amp; start",
      "Not now",
    ])
      expect(html).toContain(text);
  });
});

describe("a first-run hint's button", () => {
  const hint = (action: BoardHint["action"]): BoardHint => ({
    hint: { id: "first_review", title: "Your turn: review", text: "…", button: "Open SHOP-1" },
    anchor: { kind: "card", key: "SHOP-1" },
    action,
  });

  it("dismisses it for good, then opens the ticket or Edit flow", async () => {
    const went: string[] = [];
    const routes = {
      openTicket: (key: string) => went.push(`open ${key}`),
      editFlow: (project: string) => went.push(`edit flow ${project}`),
    };
    const { asked, ask } = door({});
    await pressHint(hint({ kind: "open_ticket", key: "SHOP-1" }), "SHOP", ask, routes);
    await pressHint(hint({ kind: "edit_flow" }), "SHOP", ask, routes);
    await pressHint(hint({ kind: "dismiss" }), "SHOP", ask, routes);
    expect(went).toEqual(["open SHOP-1", "edit flow SHOP"]);
    expect(asked.map((a) => a.params)).toEqual([
      { hint: "first_review" },
      { hint: "first_review" },
      { hint: "first_review" },
    ]);
  });
});

describe("where a first-run hint points", () => {
  /** A stand-in for an element to search: it answers its own name for each selector. */
  const within = (name: string) =>
    ({ querySelector: (selector: string) => ({ name, selector }) }) as unknown as ParentNode;

  it("finds Edit flow on the board, and the box for new work over it", () => {
    const board = within("board");
    const page = within("page");
    expect(hintTarget({ kind: "edit_flow" }, board, page)).toEqual({
      name: "board",
      selector: ".board-edit-flow",
    });
    expect(hintTarget({ kind: "new_work" }, board, page)).toEqual({
      name: "page",
      selector: ".new-work-closed",
    });
  });

  it("Edit flow from a hint closes the open ticket, so its sheet doesn't stay over the editor", () => {
    openTicket("SHOP-1");
    HINT_ROUTES.editFlow("SHOP");
    expect(selection.get().ticket).toBeNull();
    expect(flowEditing.get()).toEqual({ project: "SHOP" });
    flowEditing.set(null);
  });
});
