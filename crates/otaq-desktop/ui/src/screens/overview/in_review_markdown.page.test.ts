// @vitest-environment happy-dom
// WHAT THE AGENT DID and CHECKER on Overview are drawn as Markdown, as the same texts are in the thread (tuq 110
// refined; 0.3 regression F-C-1): inline code without its backticks, a list as a list, and each verdict line on a line
// of its own (they used to be joined with spaces).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { FilesReviewNotes } from "../../generated/FilesReviewNotes";
import { InReview } from "./InReview";

const notes = vi.hoisted((): { current: FilesReviewNotes | null } => ({ current: null }));

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

vi.mock(import("../../service"), async (importOriginal) => ({
  ...(await importOriginal()),
  useServiceView: () => ({ view: notes.current }) as never,
}));

const NOTES: FilesReviewNotes = {
  summary: {
    title: "Agent summary",
    text: "Done in two pieces:\n- Piece 1: `formatPrice` gets a comment\n- Piece 2: ran `node test.js`",
    note: "no tests are run here",
  },
  review: {
    entry: { label: "Check passed", icon: "check", palette: "done", text: null },
    report: {
      title: "Validation report",
      meta: "round 1",
      failed: false,
      summary: [
        [{ text: "Pass: **every** item is done", key: false }],
        [{ text: "Ran `node test.js`", key: false }],
      ],
      findings: [],
      checked_heading: "CHECKED",
      checked: [],
      notes_heading: "NOTES",
      notes: [],
      create: "Create ticket",
      create_icon: "plus",
    },
    foot: null,
  },
};

let host: HTMLElement;
let root: Root;

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

beforeEach(() => {
  notes.current = NOTES;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root.render(
      createElement(InReview, {
        ticket: "SHOP-4",
        view: { what_did: "What the agent did", checker: "Checker", full_report: "Full report" },
      }),
    );
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
});

const section = (label: string) => host.querySelector<HTMLElement>(`section[aria-label="${label}"]`);

describe("Overview's agent texts", () => {
  it("What the agent did draws its inline code and its list, with no raw marks", () => {
    const what = section("What the agent did");
    expect(what?.querySelector(".md-comment")).not.toBeNull();
    expect([...(what?.querySelectorAll("li") ?? [])].map((li) => li.textContent)).toEqual([
      "Piece 1: formatPrice gets a comment",
      "Piece 2: ran node test.js",
    ]);
    expect([...(what?.querySelectorAll("code") ?? [])].map((c) => c.textContent)).toEqual([
      "formatPrice",
      "node test.js",
    ]);
    expect(what?.textContent).not.toContain("`");
  });

  it("the checker's verdict keeps one line per line and draws its Markdown", () => {
    const checker = section("Checker");
    const p = checker?.querySelector(".md-comment p");
    expect(p?.innerHTML).toBe("Pass: <strong>every</strong> item is done<br>Ran <code>node test.js</code>");
    // The pill and Full report are still there.
    expect(checker?.textContent).toContain("Check passed");
    expect(checker?.textContent).toContain("Full report");
  });
});
