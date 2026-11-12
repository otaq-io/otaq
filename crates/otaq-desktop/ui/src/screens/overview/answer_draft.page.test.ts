// @vitest-environment happy-dom
// An answer's picks, ticks and Something else's text survive leaving Overview: the sheet shows one tab's body at a
// time, so Overview's answer box unmounts while Session or Changes shows and mounts again on the way back (here: the
// box is rendered, then nothing, then the box again, as the sheet's body does). A new or replaced question (a new id)
// starts from its own recommended options, a draft is its own ticket's, and a card's option click still picks, once.
// The request is mocked: what is checked is what the box shows and sends.
import { act, createElement, Fragment } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { SheetAnswer } from "../../generated/SheetAnswer";
import type { SheetView } from "../../generated/SheetView";
import { request } from "../../tauri";
import type { CardAnswer } from "../sheet/actions";
import { VIEW } from "../sheet/sheet_fixtures";
import { answerDrafts, useForgetClosedAnswer } from "./answerDraft";
import { AnswerBox } from "./NeedsYou";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => Promise.resolve({ outcome: "done" })),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

/** As the repro's "R9 f1 one": Alpha (recommended) / Beta / Something else. */
const ONE: SheetAnswer = {
  question: "q-f1",
  line: "R9 f1 one",
  context: null,
  rows: [
    { number: 1, label: "Alpha", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "Beta", recommended: false, picked: false, other: false, note: null },
    {
      number: 3,
      label: "Something else: type your own answer",
      recommended: false,
      picked: false,
      other: true,
      note: null,
    },
  ],
  recommended_label: "Recommended",
  other_placeholder: "Type your answer",
  hint: "1–3 pick · ↵ sends",
  in_terminal: "Answer in terminal",
  send: "Send answer",
};

/** As the repro's `r8c-q-multi.txt` (`--multiple`, recommended 1,3). */
const SEVERAL: SheetAnswer = {
  ...ONE,
  question: "q-multi",
  line: "Which checks?",
  rows: [
    { number: 1, label: "Unit tests", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "Lint", recommended: false, picked: false, other: false, note: null },
    { number: 3, label: "Types", recommended: true, picked: true, other: false, note: null },
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
  hint: "Pick any · 1–4 tick · ↵ sends",
};

let host: HTMLElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  answerDrafts.set(new Map());
  vi.mocked(request).mockClear();
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
});

/** The sheet: it keeps the ticket's draft to the question its view shows, and shows Overview's box or another tab. */
function Sheet({
  view,
  answer,
  overview,
  fromCard = null,
}: {
  view: SheetView;
  answer: SheetAnswer | null;
  overview: boolean;
  fromCard?: CardAnswer | null;
}) {
  useForgetClosedAnswer(view.key, answer?.question ?? null);
  return createElement(
    Fragment,
    null,
    overview && answer !== null
      ? createElement(AnswerBox, { key: answer.question, view, answer, more: null, fromCard })
      : null,
  );
}

function show(props: Parameters<typeof Sheet>[0]) {
  act(() => {
    root.render(createElement(Sheet, props));
  });
}

/** Overview → another tab → Overview. */
function leaveAndComeBack(props: Parameters<typeof Sheet>[0]) {
  show({ ...props, overview: false });
  expect(host.querySelector(".sheet-answer")).toBeNull();
  show({ ...props, overview: true });
}

const rows = () => [...host.querySelectorAll<HTMLElement>(".sheet-answer-option")];
const picked = () =>
  rows()
    .filter((r) => r.getAttribute("aria-checked") === "true")
    .map((r) => rows().indexOf(r) + 1);
const otherBox = () => host.querySelector<HTMLTextAreaElement>("textarea.sheet-answer-box");

function click(el: Element | undefined) {
  act(() => {
    (el as HTMLElement).click();
  });
}

function typeOther(text: string) {
  const box = otherBox();
  if (!box) throw new Error("no Something else field");
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set?.call(box, text);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function send() {
  await act(async () => {
    [...host.querySelectorAll("button")].find((b) => b.textContent.includes("Send answer"))?.click();
    await Promise.resolve();
  });
}

describe("an answer in the making, across the sheet's tabs", () => {
  it("keeps a question's one pick", () => {
    const props = { view: VIEW, answer: ONE, overview: true };
    show(props);
    expect(picked()).toEqual([1]);
    click(rows()[1]); // Beta
    leaveAndComeBack(props);
    expect(picked()).toEqual([2]);
  });

  it("keeps the ticks and Something else's text", async () => {
    const props = { view: VIEW, answer: SEVERAL, overview: true };
    show(props);
    expect(picked()).toEqual([1, 3]);
    click(rows()[0]); // Unit tests unticked
    click(rows()[1]); // Lint ticked
    click(rows()[3]); // Something else ticked
    typeOther("Spelling");
    leaveAndComeBack(props);
    expect(picked()).toEqual([2, 3, 4]);
    expect(otherBox()?.value).toBe("Spelling");
    await send();
    expect(request).toHaveBeenCalledWith("sheet.answer", {
      key: VIEW.key,
      question: "q-multi",
      pick: 3,
      picks: [3, 2, 4],
      other: "Spelling",
      clear: false,
    });
  });

  it("keeps Something else's text on a question that takes one answer", () => {
    const props = { view: VIEW, answer: ONE, overview: true };
    show(props);
    click(rows()[2]);
    typeOther("Gamma");
    leaveAndComeBack(props);
    expect(picked()).toEqual([3]);
    expect(otherBox()?.value).toBe("Gamma");
  });

  it("starts a new or replaced question from its own recommended options", () => {
    show({ view: VIEW, answer: ONE, overview: true });
    click(rows()[1]); // Beta
    // The agent asks again while another tab shows: the replaced question has a new id.
    show({ view: VIEW, answer: { ...ONE, question: "q-f1-again" }, overview: false });
    expect(answerDrafts.get().has(VIEW.key)).toBe(false);
    show({ view: VIEW, answer: { ...ONE, question: "q-f1-again" }, overview: true });
    expect(picked()).toEqual([1]);
    // A new question in front of you, with other options recommended.
    show({ view: VIEW, answer: SEVERAL, overview: true });
    expect(picked()).toEqual([1, 3]);
  });

  it("forgets a closed question's draft, and a sent one", async () => {
    show({ view: VIEW, answer: ONE, overview: true });
    click(rows()[1]);
    show({ view: VIEW, answer: null, overview: true });
    expect(answerDrafts.get().has(VIEW.key)).toBe(false);

    show({ view: VIEW, answer: SEVERAL, overview: true });
    click(rows()[1]);
    await send();
    expect(answerDrafts.get().has(VIEW.key)).toBe(false);
  });

  it("keeps each ticket's draft its own", () => {
    const other: SheetView = { ...VIEW, key: "SHOP-20" };
    show({ view: VIEW, answer: ONE, overview: true });
    click(rows()[1]); // Beta on SHOP-19
    // The same question id on another ticket (as a stand-in for any of its questions) opens on its recommended one.
    show({ view: other, answer: ONE, overview: true });
    expect(picked()).toEqual([1]);
    click(rows()[2]);
    typeOther("Theirs");
    show({ view: VIEW, answer: ONE, overview: true });
    expect(picked()).toEqual([2]);
    expect(otherBox()).toBeNull();
    show({ view: other, answer: ONE, overview: true });
    expect(otherBox()?.value).toBe("Theirs");
  });

  it("still picks a card's option, once: back on Overview it doesn't pick it over what was picked since", () => {
    const fromCard: CardAnswer = { question: "q-f1", kind: "pick", number: 2 };
    const props = { view: VIEW, answer: ONE, overview: true, fromCard };
    show(props);
    expect(picked()).toEqual([2]);
    click(rows()[0]); // Alpha again
    leaveAndComeBack(props);
    expect(picked()).toEqual([1]);
    // A new click on the card picks again.
    show({ ...props, fromCard: { ...fromCard } });
    expect(picked()).toEqual([2]);
  });
});
