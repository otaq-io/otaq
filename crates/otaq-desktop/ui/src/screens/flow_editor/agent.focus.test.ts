// @vitest-environment happy-dom
// The flow editor's agent (d0126, finding 51 of the 2026-10-24 dogfood report): a click anywhere in the idle panel
// puts the keyboard in its prompt, and after the first message the agent's terminal takes the keyboard as soon as it
// opens (`focusWhenLive`, `terminal/Terminal.focus.test.ts`).
import { act, createElement, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { createRegistries, RegistriesContext } from "../../app/registry";
import type { FlowEditorAgent } from "../../generated/FlowEditorAgent";
import type { TerminalProps } from "../../terminal/Terminal";
import { AgentColumn, type EditorActions, type Prefill } from "./FlowEditor";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn() }));

// The real terminal needs a canvas; this one records what it was asked.
const terminals = vi.hoisted(() => [] as { session: string; focusWhenLive: boolean }[]);
vi.mock("../../terminal/Terminal", async () => {
  const { createElement: h } = await import("react");
  return {
    Terminal: ({ session, focusWhenLive = false }: TerminalProps) => {
      terminals.push({ session, focusWhenLive });
      return h("div", { className: "fake-terminal" });
    },
  };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const IDLE: FlowEditorAgent = {
  title: "Flow agent",
  pill: { palette: "queued", icon: "circle", label: "Not started" },
  avatar: null,
  line: "Claude Code",
  note: "· starts when you send",
  agent: null,
  session: null,
  hint: ["Tell the agent how you work."],
  chips: [],
  chooser: null,
  foot: "Enter starts it",
  keys: null,
  keys_label: "",
};

const sent: string[] = [];
const writes: [string, string | null][] = [];
const actions = {
  send: (text: string, writeFor: string | null) => {
    sent.push(text);
    writes.push([text, writeFor]);
  },
} as unknown as EditorActions;
const input = createRef<HTMLInputElement>();

let root: Root;
let box: HTMLElement;

beforeEach(() => {
  terminals.length = 0;
  sent.length = 0;
  writes.length = 0;
  box = document.body.appendChild(document.createElement("div"));
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
});

function show(agent: FlowEditorAgent, prefill: Prefill | null = null) {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(AgentColumn, { agent, actions, input, terminal: null, prefill }),
      ),
    );
  });
}

it("a click beside the prompt puts the keyboard in it", () => {
  show(IDLE);
  act(() => {
    box.querySelector<HTMLElement>(".fe-agent-hint")?.click();
  });
  expect(document.activeElement).toBe(input.current);
});

it("after the first message the agent's terminal takes the keyboard once it opens", () => {
  show(IDLE);
  const prompt = input.current;
  if (!prompt) throw new Error("no prompt");
  act(() => {
    Reflect.set(HTMLInputElement.prototype, "value", "Two reviewers, then merge", prompt);
    prompt.dispatchEvent(new Event("input", { bubbles: true }));
  });
  act(() => {
    prompt.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  });
  expect(sent).toEqual(["Two reviewers, then merge"]);
  show({ ...IDLE, session: "f1" });
  expect(terminals.at(-1)).toEqual({ session: "f1", focusWhenLive: true });
});

it("a terminal the user didn't start from here doesn't take the keyboard", () => {
  show({ ...IDLE, session: "f1" });
  expect(terminals.at(-1)).toEqual({ session: "f1", focusWhenLive: false });
});

// Q441: Ask the agent to write it fills the box with the caret at the end, and sends nothing; the user's Return sends
// their finished words for that stage.
it("the write request is prefilled with the caret after it, and only the user's Return sends it", () => {
  const prefill = { stage: "plan", text: "Write a skill for the Plan stage: ", seq: 1 };
  show(IDLE, prefill);
  const prompt = input.current;
  if (!prompt) throw new Error("no prompt");
  expect(prompt.value).toBe("Write a skill for the Plan stage: ");
  expect(document.activeElement).toBe(prompt);
  expect(prompt.selectionStart).toBe(prefill.text.length);
  expect(sent).toEqual([]);
  // Re-rendered with the same press: nothing changes.
  show(IDLE, prefill);
  expect(sent).toEqual([]);
  act(() => {
    Reflect.set(HTMLInputElement.prototype, "value", `${prefill.text}list the steps first`, prompt);
    prompt.dispatchEvent(new Event("input", { bubbles: true }));
  });
  act(() => {
    prompt.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  });
  expect(writes).toEqual([["Write a skill for the Plan stage: list the steps first", "plan"]]);
});

it("a prefilled box emptied is a plain message again", () => {
  show(IDLE, { stage: "plan", text: "Write a skill for the Plan stage: ", seq: 1 });
  const prompt = input.current;
  if (!prompt) throw new Error("no prompt");
  for (const value of ["", "What does Validate do?"]) {
    act(() => {
      Reflect.set(HTMLInputElement.prototype, "value", value, prompt);
      prompt.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }
  act(() => {
    prompt.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  });
  expect(writes).toEqual([["What does Validate do?", null]]);
});
