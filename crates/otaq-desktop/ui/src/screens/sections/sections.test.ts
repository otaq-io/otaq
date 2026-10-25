import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { registered, registerScreens } from "../../app/registry";
import type { MessagesView } from "../../generated/MessagesView";
import type { ValidationReportView } from "../../generated/ValidationReportView";
import { sections } from ".";
import { MessagesBody } from "./Messages";
import { Report } from "./Report";

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

  it("registers nothing of its own: its parts are drawn by the sheet's tabs", () => {
    expect(registered(registerScreens({ sections }))).toMatchObject({
      sheetSections: [],
      sheetTabs: [],
      overlays: [],
    });
  });
});
