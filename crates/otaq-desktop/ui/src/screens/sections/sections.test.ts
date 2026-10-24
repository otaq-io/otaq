import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { registered, registerScreens } from "../../app/registry";
import type { CloseEpicDialogView } from "../../generated/CloseEpicDialogView";
import type { MessagesView } from "../../generated/MessagesView";
import type { MoveDialogView } from "../../generated/MoveDialogView";
import type { ValidationReportView } from "../../generated/ValidationReportView";
import { sections } from ".";
import { CloseEpicBody, MoveDialogBody } from "./EpicDialogs";
import { canMove, followMove, pickedChoice } from "./logic";
import { MessagesBody } from "./Messages";
import { Report } from "./Report";

const MOVE: MoveDialogView = {
  old_key: "SHOP-51",
  title: "Move SHOP-51 Wishlist share link to an epic",
  choices: [
    {
      key: "SHOP-40",
      title: "Wishlist",
      progress: "1 of 4 merged",
      new_key: "SHOP-40.4",
      heading: "Move SHOP-51 Wishlist share link to an epic",
      alias_note: "· the old key SHOP-51 still works",
      base_note: null,
      confirm: "Move to SHOP-40",
    },
    {
      key: "SHOP-60",
      title: "Gift cards",
      progress: "0 of 1 merged",
      new_key: "SHOP-60.2",
      heading: "Move SHOP-51 Wishlist share link to an epic",
      alias_note: "· the old key SHOP-51 still works",
      base_note: "SHOP-51 will start from the epic’s branch instead of release/2.3, and merge back into it.",
      confirm: "Move to SHOP-60",
    },
  ],
  disk_note: "No branch or worktree exists yet, so nothing on disk is renamed.",
  refusal: null,
};

const html = (el: Parameters<typeof renderToStaticMarkup>[0]) => renderToStaticMarkup(el);

/** A failed round's report: one finding with its place, a check, a linked note and an uncovered gap. */
export const REPORT: ValidationReportView = {
  title: "Validation report",
  meta: "round 1 · 4 min · 12k",
  failed: true,
  summary: [],
  findings: [
    {
      text: [{ text: "The description asks for keyboard navigation; arrow keys do nothing", key: false }],
      place: "components/AddressAutocomplete.tsx:88",
    },
  ],
  checked_heading: null,
  checked: [[{ text: "pnpm test passed", key: false }]],
  notes_heading: null,
  notes: [
    {
      text: [
        { text: "Order confirmation email: covered by ", key: false },
        { text: "SHOP-21", key: true },
      ],
      create: null,
    },
    {
      text: [{ text: "Not covered by any ticket: refunds endpoint", key: false }],
      create: "Not covered by any ticket: refunds endpoint",
    },
  ],
  create: "Create ticket",
  create_icon: "plus",
};

describe("the sections' interaction state", () => {
  it("Move to epic picks the first epic until one is clicked; a refusal blocks it", () => {
    expect(pickedChoice(MOVE, null)?.key).toBe("SHOP-40");
    expect(pickedChoice(MOVE, "SHOP-60")?.new_key).toBe("SHOP-60.2");
    expect(canMove(MOVE, null)).toBe(true);
    expect(canMove({ ...MOVE, refusal: "SHOP-51 can’t move" }, null)).toBe(false);
    expect(canMove({ ...MOVE, choices: [] }, null)).toBe(false);
  });

  it("the panel follows a moved ticket to its new key", () => {
    expect(followMove("SHOP-51", "SHOP-51", "SHOP-40.4")).toBe("SHOP-40.4");
    expect(followMove("SHOP-12", "SHOP-51", "SHOP-40.4")).toBeNull();
  });
});

describe("the sections", () => {
  it("Messages: the question, its answer and an Answer button on a question sent to you", () => {
    const view: MessagesView = {
      title: "Messages",
      heading: "1 thread · linked tickets only",
      you: "you",
      answer: "Answer",
      placeholder: "Type your answer",
      send: "Send",
      threads: [
        {
          id: "m1",
          answerable: true,
          question: {
            from: "SHOP-40.2",
            to: "SHOP-40.1",
            from_name: null,
            to_name: null,
            time: "10:42",
            text: "Which endpoint returns the wishlist?",
            status: "Sent to you",
            icon: "inbox",
            tone: "attention",
          },
          answer: {
            from: null,
            to: "SHOP-40.2",
            from_name: null,
            to_name: null,
            time: "10:43",
            text: "GET /api/wishlist",
            status: "Answered by you · 1 min",
            icon: "check-check",
            tone: "done",
          },
        },
      ],
    };
    const out = html(createElement(MessagesBody, { view }));
    for (const text of [
      "1 thread · linked tickets only",
      "SHOP-40.1",
      "GET /api/wishlist",
      ">you<",
      "Answer",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).toContain('data-tone="attention"');
  });

  it("Messages with the epic lead: its name at the other end (T-0290)", () => {
    const view: MessagesView = {
      title: "Messages",
      heading: "1 thread · with Wishlist lead",
      you: "you",
      answer: "Answer",
      placeholder: "Type your answer",
      send: "Send",
      threads: [
        {
          id: "q7",
          answerable: false,
          question: {
            from: "SHOP-40.2",
            to: null,
            from_name: null,
            to_name: "Wishlist lead",
            time: "11:05",
            text: "Should the button show a filled heart once the item is saved?",
            status: "Routed to the lead · not in Needs you",
            icon: "send",
            tone: "delivered",
          },
          answer: {
            from: null,
            to: "SHOP-40.2",
            from_name: "Wishlist lead",
            to_name: null,
            time: "11:06",
            text: "Yes: filled heart means saved, outline means not saved (plan, step 2).",
            status: "Answered by Wishlist lead · typed into SHOP-40.2’s session",
            icon: "check-check",
            tone: "done",
          },
        },
      ],
    };
    const out = html(createElement(MessagesBody, { view }));
    expect(out).toContain("1 thread · with Wishlist lead");
    expect(out).toContain('<span class="message-name">Wishlist lead</span>');
    expect(out).toContain("Answered by Wishlist lead · typed into SHOP-40.2’s session");
    expect(out).not.toContain(">you<");
    expect(out).not.toContain(">Answer<");
  });

  it("the Files tab's report draws its findings, checks and notes, with Create ticket", () => {
    const out = html(createElement(Report, { report: REPORT, ticket: "SHOP-32" }));
    expect(out).toContain("arrow keys do nothing");
    expect(out).toContain("components/AddressAutocomplete.tsx:88");
    expect(out).toContain("Create ticket");
    expect(out).toContain('class="report-key"');
  });

  it("a passed report's verdict shows with a check, not as a ✕ finding (dogfood #17)", () => {
    const passed: ValidationReportView = {
      ...REPORT,
      failed: false,
      summary: [[{ text: "Pass: every item in the description is done", key: false }]],
      findings: [],
      checked_heading: "CHECKED",
      notes_heading: "NOTES",
    };
    const out = html(createElement(Report, { report: passed, ticket: "SHOP-18" }));
    expect(
      /<div class="report-line" data-tone="(\w+)">(?:(?!report-line).)*Pass: every item/.exec(out)?.[1],
    ).toBe("checked");
    expect(out).not.toContain('data-tone="finding"');
  });

  it("the epic dialogs: the new key, the base note, the agents and what stays merged", () => {
    const move = html(
      createElement(MoveDialogBody, { view: MOVE, picked: "SHOP-60", onPick: () => undefined, error: null }),
    );
    for (const text of [
      "SHOP-60.2",
      "the old key SHOP-51 still works",
      "instead of release/2.3",
      MOVE.disk_note,
    ]) {
      expect(move).toContain(text);
    }
    const refused = html(
      createElement(MoveDialogBody, {
        view: { ...MOVE, refusal: "SHOP-51 can’t move" },
        picked: null,
        onPick: () => undefined,
        error: null,
      }),
    );
    expect(refused).toContain("SHOP-51 can’t move");

    const close: CloseEpicDialogView = {
      title: "Close epic SHOP-40 Wishlist?",
      agents: ["SHOP-40.2 Wishlist button — its agent is running."],
      also: "Also closes SHOP-40.3 Wishlist page (Backlog).",
      note: "Branches are kept.",
      confirm: "Stop 1 agent & close 2 tickets",
    };
    const out = html(createElement(CloseEpicBody, { view: close, error: null }));
    for (const text of [close.agents[0] ?? "", close.also ?? "", close.note]) expect(out).toContain(text);
  });

  it("registers the sections and the epic dialogs from its folder", () => {
    expect(registered(registerScreens({ sections }))).toMatchObject({
      panelSections: [],
      sheetTabs: [],
      overlays: ["move_to_epic", "close_epic"],
    });
  });
});
