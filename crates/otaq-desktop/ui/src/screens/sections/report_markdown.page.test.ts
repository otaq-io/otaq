// @vitest-environment happy-dom
// The Validation report keeps its structure (findings with their place, CHECKED, NOTES, Create ticket) and draws each
// line's words as inline Markdown (tuq 110 refined; 0.3 regression F-C-1). Ticket keys Rust found stay buttons that
// open their ticket, inside inline code too; Create ticket sends the note as written, not as drawn.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { ValidationReportView } from "../../generated/ValidationReportView";
import { Report } from "./Report";

const calls = vi.hoisted(() => ({ requests: [] as [string, unknown][], opened: [] as string[] }));

vi.mock("../../tauri", () => ({
  request: (method: string, params: unknown) => {
    calls.requests.push([method, params]);
    return Promise.resolve(null);
  },
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

vi.mock(import("../../app/selection"), async (importOriginal) => ({
  ...(await importOriginal()),
  selectProject: () => undefined,
  openTicket: (key: string) => {
    calls.opened.push(key);
  },
}));

const NOTE = "Not covered by any ticket: the `refunds` endpoint";

const FAILED: ValidationReportView = {
  title: "Validation report",
  meta: "round 1",
  failed: true,
  summary: [],
  findings: [
    {
      text: [{ text: "`formatPrice` drops the **cents**", key: false }],
      place: "src/price.ts:42",
    },
    { text: [{ text: "# not a heading, 1. not a list", key: false }], place: null },
  ],
  checked_heading: null,
  checked: [],
  notes_heading: null,
  notes: [],
  create: "Create ticket",
  create_icon: "plus",
};

const PASSED: ValidationReportView = {
  title: "Validation report",
  meta: "round 2",
  failed: false,
  summary: [
    [
      { text: "Pass: covered by `", key: false },
      { text: "SHOP-21", key: true },
      { text: "` and ", key: false },
      { text: "SHOP-22", key: true },
      { text: ".", key: false },
    ],
  ],
  findings: [],
  checked_heading: "CHECKED",
  checked: [[{ text: "ran `node test.js`", key: false }]],
  notes_heading: "NOTES",
  notes: [
    { text: [{ text: "Order email is *already* done", key: false }], create: null },
    { text: [{ text: NOTE, key: false }], create: NOTE },
  ],
  create: "Create ticket",
  create_icon: "plus",
};

let host: HTMLElement;
let root: Root;

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

beforeEach(() => {
  calls.requests.length = 0;
  calls.opened.length = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
});

function draw(report: ValidationReportView): void {
  act(() => {
    root.render(createElement(Report, { report, ticket: "SHOP-4" }));
  });
}

const lines = (tone: string) => [...host.querySelectorAll(`.report-line[data-tone="${tone}"]`)];

describe("the Validation report's lines", () => {
  it("a finding draws its inline Markdown and keeps its place", () => {
    draw(FAILED);
    const [first, second] = lines("finding");
    expect(first?.querySelector(".report-md")?.innerHTML).toBe(
      "<code>formatPrice</code> drops the <strong>cents</strong>",
    );
    expect(first?.querySelector(".report-place")?.textContent).toBe("src/price.ts:42");
    // A line is one line: what would start a heading or a list stays its text.
    expect(second?.querySelector(".report-md")?.textContent).toBe("# not a heading, 1. not a list");
    expect(second?.querySelector("h1, ol, ul, p")).toBeNull();
  });

  it("a ticket key inside inline code is still a button that opens the ticket, with no stray backticks", () => {
    draw(PASSED);
    const [summary] = lines("checked");
    const md = summary?.querySelector(".report-md");
    expect(md?.textContent).toBe("Pass: covered by SHOP-21 and SHOP-22.");
    const inCode = md?.querySelector<HTMLButtonElement>("code > button.report-key");
    expect(inCode?.textContent).toBe("SHOP-21");
    const plain = [...(md?.querySelectorAll<HTMLButtonElement>("button.report-key") ?? [])].map(
      (b) => b.textContent,
    );
    expect(plain).toEqual(["SHOP-21", "SHOP-22"]);
    act(() => {
      inCode?.click();
    });
    expect(calls.opened).toEqual(["SHOP-21"]);
  });

  it("a key opens only as a whole word, and a link's words stay the link's", () => {
    draw({
      ...PASSED,
      summary: [
        [
          { text: "SHOP-2", key: true },
          { text: " first, SHOP-21 is another key, then ", key: false },
          { text: "SHOP-2", key: true },
          { text: ". See [SHOP-2 run](https://e.com/run)", key: false },
        ],
      ],
    });
    const md = lines("checked")[0]?.querySelector(".report-md");
    expect([...(md?.querySelectorAll("button.report-key") ?? [])].map((b) => b.textContent)).toEqual([
      "SHOP-2",
      "SHOP-2",
    ]);
    expect(md?.querySelector("a")?.textContent).toBe("SHOP-2 run");
    expect(md?.querySelector("a button")).toBeNull();
    expect(md?.textContent).toBe("SHOP-2 first, SHOP-21 is another key, then SHOP-2. See SHOP-2 run");
  });

  it("CHECKED and NOTES keep their headings and draw their lines' Markdown", () => {
    draw(PASSED);
    expect([...host.querySelectorAll(".report-heading")].map((h) => h.textContent)).toEqual([
      "CHECKED",
      "NOTES",
    ]);
    expect(lines("checked")[1]?.querySelector("code")?.textContent).toBe("node test.js");
    expect(lines("note")[0]?.querySelector("em")?.textContent).toBe("already");
  });

  it("a note keeps Create ticket, which sends the note as written", () => {
    draw(PASSED);
    const note = lines("note")[1];
    expect(note?.querySelector("code")?.textContent).toBe("refunds");
    const create = [...(note?.querySelectorAll("button") ?? [])].find(
      (b) => b.textContent === "Create ticket",
    );
    expect(create).toBeDefined();
    act(() => {
      create?.click();
    });
    expect(calls.requests).toEqual([["sections.create_from_note", { key: "SHOP-4", note: NOTE }]]);
  });
});
