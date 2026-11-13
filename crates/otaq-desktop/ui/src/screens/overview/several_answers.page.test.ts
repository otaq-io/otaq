// @vitest-environment happy-dom
// A question that takes several answers draws checkboxes, the recommended ones ticked; a click or a number key ticks
// or unticks a row, Other… among them; Send answer needs at least one ticked and sends every ticked row. The same box
// on a ticket's Overview (`sheet.answer`) and for Noyan (`agent_panel.answer`). A question that takes one answer keeps
// its radios and its one pick.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentQuestion } from "../../generated/AgentQuestion";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import { lettersKeptBy } from "../../app/keys";
import { request } from "../../tauri";
import { AgentQuestionBox } from "../agent_panel/AgentQuestion";
import { VIEW } from "../sheet/sheet_fixtures";
import { AnswerBox } from "./NeedsYou";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => Promise.resolve({ outcome: "done" })),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

/** As `otaq-view` draws `ask human --multiple --recommended 1,3`. */
const SEVERAL: SheetAnswer = {
  question: "q-providers",
  line: "Which email providers should the fallback chain use?",
  context: null,
  rows: [
    { number: 1, label: "Postmark", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "SES", recommended: false, picked: false, other: false, note: null },
    { number: 3, label: "Resend", recommended: true, picked: true, other: false, note: null },
    {
      number: 4,
      label: "Something else: type your own answer",
      recommended: false,
      picked: false,
      other: true,
      note: null,
    },
  ],
  multiple: true,
  recommended_label: "Recommended",
  other_placeholder: "Type your answer",
  hint: "Pick any · 1–4 tick · ↵ sends",
  in_terminal: "Answer in terminal",
  send: "Send answer",
};

/** A question that takes one answer, as every question stored before: no `multiple`. */
const ONE: SheetAnswer = {
  ...SEVERAL,
  question: "q-one",
  rows: SEVERAL.rows.map((r) => ({ ...r, recommended: r.number === 1, picked: r.number === 1 })),
  multiple: undefined,
  hint: "1–4 pick · ↵ sends",
};

let host: HTMLElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.mocked(request).mockClear();
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
});

const rows = () => [...host.querySelectorAll<HTMLElement>(".sheet-answer-option")];
const ticked = () =>
  rows()
    .filter((r) => r.getAttribute("aria-checked") === "true")
    .map((r) => rows().indexOf(r) + 1);
const sendButton = () =>
  [...host.querySelectorAll("button")].find((b) => b.textContent.includes("Send answer"));

function click(el: Element | undefined) {
  act(() => {
    (el as HTMLElement).click();
  });
}

function key(n: string) {
  const list = host.querySelector<HTMLElement>(".sheet-answer-options");
  act(() => {
    list?.dispatchEvent(new KeyboardEvent("keydown", { key: n, bubbles: true }));
  });
}

function typeOther(text: string) {
  const box = host.querySelector<HTMLTextAreaElement>("textarea.sheet-answer-box");
  if (!box) throw new Error("no Other… field");
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set?.call(box, text);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function send() {
  await act(async () => {
    sendButton()?.click();
    await Promise.resolve();
  });
}

describe("a question that takes several answers, on a ticket's Overview", () => {
  it("ticks two options and Other…, and sends them all", async () => {
    act(() => {
      root.render(createElement(AnswerBox, { view: VIEW, answer: SEVERAL, more: null, fromCard: null }));
    });
    const list = host.querySelector(".sheet-answer-options");
    expect(list?.getAttribute("role")).toBe("group");
    expect(rows().every((r) => r.getAttribute("role") === "checkbox")).toBe(true);
    expect(host.querySelectorAll(".sheet-answer-check")).toHaveLength(4);
    expect(host.querySelectorAll(".sheet-answer-radio")).toHaveLength(0);
    expect(ticked()).toEqual([1, 3]);
    expect(host.querySelectorAll(".sheet-answer-rec")).toHaveLength(2);

    click(rows()[2]); // Resend unticked
    key("2"); // SES ticked
    click(rows()[3]); // Other… ticked
    expect(ticked()).toEqual([1, 2, 4]);
    expect(sendButton()?.hasAttribute("disabled")).toBe(true);
    typeOther("Mailgun for receipts");
    expect(sendButton()?.hasAttribute("disabled")).toBe(false);
    await send();
    expect(request).toHaveBeenCalledWith("sheet.answer", {
      key: VIEW.key,
      question: "q-providers",
      pick: 1,
      picks: [1, 2, 4],
      other: "Mailgun for receipts",
      clear: false,
    });
  });

  it("keeps its number keys from the sheet's 1–4 tabs, as a question that takes one answer does (NEW-b3)", () => {
    for (const answer of [SEVERAL, ONE]) {
      act(() => {
        root.render(createElement(AnswerBox, { view: VIEW, answer, more: null, fromCard: null }));
      });
      // A clicked row, and the list a card's option focuses.
      for (const focus of [rows()[1], host.querySelector<HTMLElement>(".sheet-answer-options")]) {
        act(() => {
          focus?.focus();
        });
        expect(document.activeElement).toBe(focus);
        expect(lettersKeptBy(document.activeElement as HTMLElement), answer.question).toBe("digits");
      }
    }
  });

  it("needs at least one ticked", () => {
    act(() => {
      root.render(createElement(AnswerBox, { view: VIEW, answer: SEVERAL, more: null, fromCard: null }));
    });
    key("1");
    key("3");
    expect(ticked()).toEqual([]);
    expect(sendButton()?.hasAttribute("disabled")).toBe(true);
  });

  it("a question that takes one answer keeps its radios and sends its one pick", async () => {
    act(() => {
      root.render(createElement(AnswerBox, { view: VIEW, answer: ONE, more: null, fromCard: null }));
    });
    expect(rows().every((r) => r.getAttribute("role") === "radio")).toBe(true);
    key("2");
    key("3");
    expect(ticked()).toEqual([3]);
    await send();
    expect(request).toHaveBeenCalledWith("sheet.answer", {
      key: VIEW.key,
      question: "q-one",
      pick: 3,
      other: "",
      clear: false,
    });
  });
});

describe("a question that takes several answers, from Noyan", () => {
  it("number keys tick, and the answer goes with every ticked row", async () => {
    const question: AgentQuestion = {
      project: "NOTE",
      agent: "a1",
      title: "Noyan · Providers asks",
      answer: SEVERAL,
    };
    act(() => {
      root.render(createElement(AgentQuestionBox, { question, onTerminal: () => undefined }));
    });
    key("1"); // Postmark unticked
    key("2"); // SES ticked
    expect(ticked()).toEqual([2, 3]);
    await send();
    expect(request).toHaveBeenCalledWith("agent_panel.answer", {
      project: "NOTE",
      agent: "a1",
      question: "q-providers",
      pick: 3,
      picks: [3, 2],
      other: "",
      clear: false,
    });
  });
});
