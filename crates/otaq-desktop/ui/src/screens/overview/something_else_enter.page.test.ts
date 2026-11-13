// @vitest-environment happy-dom
// ↵ in Something else's field sends the answer once, on a ticket's Overview and in Noyan's box (the same answer
// options; the strip's popover and Edit flow's box draw Noyan's): the field's own ↵ sends and its list's keys leave
// it alone, so a digit types into it instead of picking a row. ↵ and 1–4 on a focused row still pick, tick and send;
// ⇧↵ makes a new line. The request is mocked: what is checked is how many answers the box sends.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentQuestion } from "../../generated/AgentQuestion";
import type { SheetAnswer } from "../../generated/SheetAnswer";
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

/** As the repro's "R9 f1 one": Alpha / Beta / Something else. */
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

const SEVERAL: SheetAnswer = {
  ...ONE,
  question: "q-several",
  multiple: true,
  hint: "Pick any · 1–3 tick · ↵ sends",
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
const picked = () =>
  rows()
    .filter((r) => r.getAttribute("aria-checked") === "true")
    .map((r) => rows().indexOf(r) + 1);
const field = () => host.querySelector<HTMLTextAreaElement>("textarea.sheet-answer-box");
const answers = () => vi.mocked(request).mock.calls.filter(([method]) => method.endsWith(".answer"));

/** A key pressed on `el`, bubbling as a real one does; whether the app took it (its default prevented). */
async function press(
  el: Element | null | undefined,
  key: string,
  init: KeyboardEventInit = {},
): Promise<boolean> {
  if (!el) throw new Error("nothing to press on");
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  await act(async () => {
    el.dispatchEvent(event);
    await Promise.resolve();
  });
  return event.defaultPrevented;
}

function type(text: string) {
  const box = field();
  if (!box) throw new Error("no Something else field");
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set?.call(box, text);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function somethingElse(text: string) {
  act(() => {
    rows()[2]?.click();
  });
  expect(document.activeElement).toBe(field());
  type(text);
}

const BOXES = [
  {
    name: "a ticket's Overview",
    method: "sheet.answer",
    render: (answer: SheetAnswer) =>
      createElement(AnswerBox, { view: VIEW, answer, more: null, fromCard: null }),
  },
  {
    name: "Noyan's box (the panel, the strip's popover, Edit flow)",
    method: "agent_panel.answer",
    render: (answer: SheetAnswer) => {
      const question: AgentQuestion = { project: "NOTE", agent: "a1", title: "Noyan · asks", answer };
      return createElement(AgentQuestionBox, { question, onTerminal: () => undefined });
    },
  },
];

describe.each(BOXES)("Something else, in $name", ({ method, render }) => {
  it("↵ in its field sends the answer once (NEW-f1)", async () => {
    act(() => {
      root.render(render(ONE));
    });
    somethingElse("Go on");
    expect(await press(field(), "Enter")).toBe(true);
    expect(answers()).toHaveLength(1);
    expect(answers()[0]?.[0]).toBe(method);
    expect(answers()[0]?.[1]).toMatchObject({ pick: 3, other: "Go on" });
  });

  it("a digit typed in its field types, not picks a row", async () => {
    act(() => {
      root.render(render(ONE));
    });
    somethingElse("V");
    expect(await press(field(), "2")).toBe(false);
    expect(picked()).toEqual([3]);
    expect(field()).not.toBeNull();
    expect(answers()).toHaveLength(0);
  });

  it("⇧↵ in its field makes a new line, and sends nothing", async () => {
    act(() => {
      root.render(render(ONE));
    });
    somethingElse("Go on");
    expect(await press(field(), "Enter", { shiftKey: true })).toBe(false);
    expect(answers()).toHaveLength(0);
  });

  it("↵ and a digit on a focused row still pick and send, once", async () => {
    act(() => {
      root.render(render(ONE));
    });
    expect(await press(rows()[0], "2")).toBe(true);
    expect(picked()).toEqual([2]);
    expect(await press(rows()[1], "Enter")).toBe(true);
    expect(answers()).toHaveLength(1);
    expect(answers()[0]?.[1]).toMatchObject({ pick: 2 });
  });

  it("a digit on a focused row ticks it, on a question that takes several answers", async () => {
    act(() => {
      root.render(render(SEVERAL));
    });
    expect(picked()).toEqual([1]);
    await press(rows()[0], "2");
    expect(picked()).toEqual([1, 2]);
    await press(rows()[0], "1");
    expect(picked()).toEqual([2]);
    await press(rows()[1], "Enter");
    expect(answers()).toHaveLength(1);
    expect(answers()[0]?.[1]).toMatchObject({ picks: [2] });
  });

  it("two sends before the box draws again send once", async () => {
    act(() => {
      root.render(render(ONE));
    });
    const button = [...host.querySelectorAll("button")].find((b) => b.textContent.includes("Send answer"));
    await act(async () => {
      button?.click();
      button?.click();
      await Promise.resolve();
    });
    expect(answers()).toHaveLength(1);
  });
});
