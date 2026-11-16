// @vitest-environment happy-dom
// A question a restart stopped (re-test 0.3 F-U2-1): Reply opens the ticket on Overview with its question, and the box
// sends with "Answer & resume" (the answer resumes the conversation). The Interrupted bar (Resume) and a held tool's
// callout stay under the box, so what would make the answer fail still shows. A live Asks you's box stands alone.
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

const LIVE: SheetAnswer = {
  question: "q-licence",
  line: "Which licence?",
  context: null,
  rows: [
    { number: 1, label: "MIT", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "Apache-2.0", recommended: false, picked: false, other: false, note: null },
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

const INTERRUPTED: SheetView = {
  ...VIEW,
  key: "NOTE-1",
  answer: { ...LIVE, send: "Answer & resume", resumes: true },
  buttons: [{ action: "resume", label: "Resume", tooltip: null, icon: "play", look: "primary" }],
  holds: [
    {
      role: "developer",
      tone: "error",
      avatar: null,
      heading: "Claude Code is signed out",
      text: "Sign in, then answer.",
      model: null,
      buttons: [],
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

function render(view: SheetView) {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(NeedsYou, { view, actions: ACTIONS }),
      ),
    );
  });
}

function buttonLabels(): string[] {
  return [...host.querySelectorAll("button")].map((b) => b.textContent.trim());
}

describe("a question a restart stopped", () => {
  it("shows its box with Answer & resume, and the Interrupted bar and callouts stay under it", () => {
    render(INTERRUPTED);
    const box = host.querySelector(".sheet-answer");
    expect(box).not.toBeNull();
    expect(host.textContent).toContain("Which licence?");
    expect(buttonLabels()).toContain("Answer & resume");
    expect(buttonLabels()).not.toContain("Send answer");
    const bar = [...host.querySelectorAll(".sheet-bar")].find(
      (b) => b.textContent.includes("Resume") && !b.textContent.includes("Answer"),
    );
    const hold = host.querySelector(".sheet-holds");
    expect(bar).toBeDefined();
    expect(hold?.textContent).toContain("Claude Code is signed out");
    // The box comes first.
    expect(box && bar && box.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(box && hold && box.compareDocumentPosition(hold) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("a live Asks you's box stands alone with Send answer", () => {
    render({ ...INTERRUPTED, answer: LIVE });
    expect(host.querySelector(".sheet-answer")).not.toBeNull();
    expect(buttonLabels()).toContain("Send answer");
    expect(buttonLabels()).not.toContain("Resume");
    expect(host.querySelector(".sheet-holds")).toBeNull();
  });
});
