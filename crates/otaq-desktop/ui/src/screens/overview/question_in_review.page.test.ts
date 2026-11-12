// @vitest-environment happy-dom
// An agent's question while the ticket waits in a check of yours (re-test 8 NEW-b2): the Overview shows its box first,
// and the review's own rows and bar stay under it, so Approve, Send back and Move keep working (the review bar under every tab) while it is open. At
// work an Asks you's box stands alone, as before.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { createRegistries, RegistriesContext } from "../../app/registry";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import type { SheetView } from "../../generated/SheetView";
import type { SheetActions } from "../sheet/actions";
import { VIEW } from "../sheet/sheet_fixtures";
import { NeedsYou } from "./NeedsYou";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const ANSWER: SheetAnswer = {
  question: "q-review",
  line: "Should this show?",
  context: "",
  rows: [
    { number: 1, label: "Yes", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "No", recommended: false, picked: false, other: false, note: null },
    { number: 3, label: "Something else", recommended: false, picked: false, other: true, note: null },
  ],
  recommended_label: "Recommended",
  other_placeholder: "Type your answer",
  hint: "1–3 pick · ↵ sends",
  in_terminal: "Answer in terminal",
  send: "Send answer",
};

const ACTIONS: SheetActions = {
  press: () => undefined,
  choose: () => undefined,
  busy: null,
  answering: null,
  setAnswering: () => undefined,
  cardAnswer: null,
};

const VIEW_ASKING: SheetView = {
  ...VIEW,
  key: "SHOP-13",
  answer: ANSWER,
  buttons: [
    {
      action: "open_terminal",
      label: "Open agent session",
      tooltip: null,
      icon: "square-terminal",
      look: "secondary",
    },
  ],
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
});

function render(inReview: boolean) {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(NeedsYou, {
          view: VIEW_ASKING,
          actions: ACTIONS,
          aboveBar: createElement("p", { className: "manual-checks-stand-in" }, "Manual checks for you"),
          inReview,
        }),
      ),
    );
  });
}

describe("a question in your check", () => {
  it("shows its box above the review's rows and bar, which stay", () => {
    render(true);
    const box = host.querySelector(".sheet-answer");
    const checks = host.querySelector(".manual-checks-stand-in");
    // The review's own bar, not the box's (Send answer, Answer in terminal).
    const bar = [...host.querySelectorAll(".sheet-bar")].find((b) =>
      b.textContent.includes("Open agent session"),
    );
    expect(box).not.toBeNull();
    expect(checks).not.toBeNull();
    expect(bar).toBeDefined();
    // The box comes first.
    expect(
      box && checks && box.compareDocumentPosition(checks) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(box && bar && box.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("at work, an Asks you's box stands alone", () => {
    render(false);
    expect(host.querySelector(".sheet-answer")).not.toBeNull();
    expect(host.textContent).not.toContain("Open agent session");
    expect(host.querySelector(".manual-checks-stand-in")).toBeNull();
  });
});
