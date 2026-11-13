// @vitest-environment happy-dom
// A long agent question reads whole and keeps Send in view: its first line and its context are one text that scrolls
// together, the options and Send outside it. On a ticket's Overview the text caps at the room the options and Send leave
// only when Send would fall below the sheet; above Noyan's terminal the band caps at 55% of the panel, the terminal keeping at
// least 200 px.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentQuestion } from "../../generated/AgentQuestion";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import { AgentQuestionBox } from "../agent_panel/AgentQuestion";
import { VIEW } from "../sheet/sheet_fixtures";
import { AnswerBox } from "./NeedsYou";
import { QUESTION_FLOOR } from "./QuestionText";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

/** The brief's long question, as `otaq-view` splits it. */
const LONG: SheetAnswer = {
  question: "q-security-review",
  line: "Which agent should run the new Security review stage?",
  context: [
    "Your project has a security skill in `.claude/skills/security-review`, and it expects the code to build first.",
    "",
    "- **Verifier** already runs Validate on Codex.",
    "- **Strong** (Claude Code) loads your skill as it is.",
    "",
    "A third agent costs a new connection in Settings → Agents.",
  ].join("\n"),
  rows: [
    { number: 1, label: "Strong", recommended: true, picked: true, other: false, note: "Uses your skill." },
    { number: 2, label: "Verifier", recommended: false, picked: false, other: false, note: "One checker." },
    { number: 3, label: "Something else", recommended: false, picked: false, other: true, note: null },
  ],
  recommended_label: "Recommended",
  other_placeholder: "Type your answer",
  hint: "1–4 pick · ↵ sends",
  in_terminal: "Answer in terminal",
  send: "Send answer",
};

let host: HTMLElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  vi.restoreAllMocks();
  document.head.innerHTML = "";
});

const sendButton = (scope: Element) =>
  [...scope.querySelectorAll("button")].find((b) => b.textContent.includes("Send answer"));

/** The question's line and context sit in its one scrolling text; the options and Send don't. */
function expectOneScrollingText(scope: Element): HTMLElement {
  const text = scope.querySelector<HTMLElement>(".sheet-answer-text");
  if (!text) throw new Error("no question text");
  expect(text.querySelector(".sheet-answer-line")?.textContent).toBe(LONG.line);
  expect(text.querySelector(".sheet-answer-context")?.textContent).toContain("A third agent costs");
  expect(text.querySelectorAll(".sheet-answer-context li")).toHaveLength(2);
  expect(text.querySelector(".sheet-answer-context code")?.textContent).toBe(
    ".claude/skills/security-review",
  );
  const send = sendButton(scope);
  expect(send).toBeDefined();
  expect(text.contains(send ?? null)).toBe(false);
  expect(text.contains(scope.querySelector(".sheet-answer-options"))).toBe(false);
  return text;
}

/** Heights by class, the way the page would lay them out: the sheet's body `body` tall, the text `whole` tall. */
function layout(body: number, whole: number) {
  const height = (el: Element, prop: "offsetHeight" | "clientHeight" | "scrollHeight"): number => {
    if (el.classList.contains("sheet-body")) return body;
    if (el.classList.contains("sheet-answer-text")) {
      if (prop === "scrollHeight") return whole;
      const cap = Number.parseFloat((el as HTMLElement).style.maxHeight);
      return Number.isNaN(cap) ? whole : Math.min(cap, whole);
    }
    if (el.classList.contains("sheet-question")) {
      const text = el.querySelector(".sheet-answer-text");
      return 260 + (text ? height(text, "clientHeight") : 0);
    }
    return 0;
  };
  for (const prop of ["offsetHeight", "clientHeight", "scrollHeight"] as const) {
    vi.spyOn(HTMLElement.prototype, prop, "get").mockImplementation(function (this: HTMLElement) {
      return height(this, prop);
    });
  }
}

function renderOverview() {
  act(() => {
    root.render(
      createElement(
        "div",
        { className: "sheet-body" },
        createElement(AnswerBox, { view: VIEW, answer: LONG, more: null, fromCard: null }),
      ),
    );
  });
}

describe("a long question on a ticket's Overview", () => {
  it("has no cap of its own while Send fits in the sheet", () => {
    layout(900, 400);
    renderOverview();
    const text = expectOneScrollingText(host);
    expect(text.style.maxHeight).toBe("");
  });

  it("caps its text at the room the options and Send leave when Send would fall below the sheet", () => {
    // The rest of the box (options with notes, Something else, the hint and Send) is 260 px: the text gets the
    // other 340 px of a 600-px body, not half of it, and the box ends at the body's bottom edge (NEW-c1).
    layout(600, 900);
    renderOverview();
    const text = expectOneScrollingText(host);
    expect(text.style.maxHeight).toBe("340px");
    expect(host.querySelector<HTMLElement>(".sheet-question")?.style.maxHeight).toBe("600px");
  });

  it("keeps the text's floor in a very small body, the box still ending at its bottom so the options scroll", () => {
    layout(300, 900);
    renderOverview();
    const text = expectOneScrollingText(host);
    expect(text.style.maxHeight).toBe(`${String(QUESTION_FLOOR)}px`);
    expect(host.querySelector<HTMLElement>(".sheet-question")?.style.maxHeight).toBe("300px");
  });

  it("measures the options whole while they scroll, so a cap doesn't feed the next measure", () => {
    // The options scroll 40 px of their 100: the rest of the box counts them whole, and the text's cap stays.
    layout(600, 900);
    vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) {
      if (this.classList.contains("sheet-answer-options")) return 40;
      return this.classList.contains("sheet-answer-text") ? 900 : 0;
    });
    renderOverview();
    expect(host.querySelector<HTMLElement>(".sheet-answer-text")?.style.maxHeight).toBe("300px");
  });
});

describe("a long question above Noyan's terminal", () => {
  it("caps its band at 55% of the panel, the terminal keeping 200 px; the text scrolls, Send stays out of it", async () => {
    // The page's own style sheets (the test runner leaves CSS imports empty).
    const fsModule = "node:fs";
    const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
    const here = (import.meta as ImportMeta & { dirname: string }).dirname;
    for (const file of ["../sheet/actions.css", "../agent_panel/agent_panel.css"]) {
      const style = document.createElement("style");
      style.textContent = fs.readFileSync(`${here}/${file}`, "utf8");
      document.head.append(style);
    }
    const question: AgentQuestion = {
      project: "SHOP",
      agent: "a1",
      title: "Noyan · Security asks",
      answer: LONG,
    };
    act(() => {
      root.render(
        createElement(
          "div",
          { className: "agent-panel-body" },
          createElement(
            "div",
            { className: "agent-panel-question" },
            createElement(AgentQuestionBox, { question, onTerminal: () => undefined }),
          ),
          createElement("div", { className: "agent-panel-well" }),
        ),
      );
    });
    const band = host.querySelector<HTMLElement>(".agent-panel-question");
    if (!band) throw new Error("no band");
    const text = expectOneScrollingText(band);
    expect(getComputedStyle(band).maxHeight).toBe("55%");
    expect(getComputedStyle(band).display).toBe("flex");
    expect(getComputedStyle(text).overflowY).toBe("auto");
    expect(Number.parseFloat(getComputedStyle(text).minHeight)).toBe(0);
    const options = band.querySelector(".sheet-answer-options");
    const bar = band.querySelector(".agent-question-bar");
    // The text gives way first (down to its floor), then the options scroll; the bar never gives way (NEW-d1, NEW-d2).
    expect(Number(getComputedStyle(text).flexShrink)).toBeGreaterThanOrEqual(1000);
    expect(options && getComputedStyle(options).flexShrink).toBe("1");
    expect(options && getComputedStyle(options).overflowY).toBe("auto");
    expect(options && Number.parseFloat(getComputedStyle(options).minHeight)).toBe(0);
    expect(bar && getComputedStyle(bar).flexShrink).toBe("0");
    const well = host.querySelector(".agent-panel-well");
    expect(well && getComputedStyle(well).minHeight).toBe("200px");
    // The context's lines are about 60 characters here (70 on a ticket's Overview).
    const context = band.querySelector(".sheet-answer-context");
    expect(context && getComputedStyle(context).maxWidth).toBe("340px");
  });
});

describe("a long question whose options have notes, in a tight place (NEW-d1, NEW-d2)", () => {
  /** The question's words `words` px tall, as the page would draw them. */
  function words(height: number) {
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("sheet-answer-question") ? height : 0;
    });
  }
  const question: AgentQuestion = {
    project: "SHOP",
    agent: "a1",
    title: "Noyan · Security asks",
    answer: LONG,
  };

  it("keeps about six lines of the question in view, the options scrolling instead", () => {
    words(400);
    act(() => {
      root.render(createElement(AgentQuestionBox, { question, onTerminal: () => undefined }));
    });
    expect(host.querySelector<HTMLElement>(".sheet-answer-text")?.style.minHeight).toBe(
      `${String(QUESTION_FLOOR)}px`,
    );
  });

  it("keeps a short question whole, with no room left under it", () => {
    words(46);
    act(() => {
      root.render(createElement(AgentQuestionBox, { question, onTerminal: () => undefined }));
    });
    expect(host.querySelector<HTMLElement>(".sheet-answer-text")?.style.minHeight).toBe("46px");
  });
});
