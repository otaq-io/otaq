// @vitest-environment happy-dom
// A long question in the Needs you strip opens wide: its popover is 500 px wide and caps at 560 px tall, the question
// drawn by the shared question box, its text scrolling inside the cap while the options, Send and the way to the
// larger place stay in view. Open in the agent panel (Noyan's question) and Answer 40.2… (a helper's, on its ticket)
// land on the question, shown whole. A short question's popover fits its content.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, type ShellLayout } from "../../app/layout";
import { createSelection, type TerminalFocusRequest } from "../../app/selection";
import { createStore } from "../../app/store";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import type { StripChip } from "../../generated/StripChip";
import type { StripTarget } from "../../generated/StripTarget";
import type { StripView } from "../../generated/StripView";
import type { AgentPanelFocusRequest, ShownAgents } from "../agent_panel/logic";
import { openTarget, type StripRoutes } from "./logic";
import { StripBody } from "./Strip";

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

const AGENT: StripTarget = { kind: "agent", project: "SHOP", agent: "pa3" };
const OVERVIEW: StripTarget = {
  kind: "ticket",
  project: "SHOP",
  key: "SHOP-40",
  tab: "overview",
  piece: null,
};
const HELPER: StripTarget = { kind: "ticket", project: "SHOP", key: "SHOP-40", tab: "agent", piece: "40.2" };

/** Noyan's chip, as `needs_you_strip.rs` draws it, asking `answer`. */
function noyan(answer: SheetAnswer): StripChip {
  return {
    label: "Agent",
    reason: "a question",
    icon: "bot",
    tone: "investigate",
    title: `Noyan · Security · asks: ${answer.line}`,
    target: AGENT,
    asks: null,
    agent_asks: { project: "SHOP", agent: "pa3", title: "Noyan · Security asks", answer },
    open_in_panel: "Open in the agent panel",
  };
}

/** A helper's passed-on question on SHOP-40. */
function helper(line: string, context: string | null): StripChip {
  return {
    label: "SHOP-40",
    reason: "40.2 asks",
    icon: "message-circle-question-mark",
    tone: "attn",
    title: `SHOP-40 Wishlist · 40.2 asks: ${line}`,
    target: HELPER,
    asks: {
      title: "SHOP-40 · 40.2 asks",
      line,
      context,
      note: "The agent in charge passed this to you: it’s a product question the plan doesn’t answer.",
      answer: "Answer 40.2…",
      answer_target: OVERVIEW,
      open: "Open in the ticket",
      open_target: HELPER,
    },
    agent_asks: null,
    open_in_panel: null,
  };
}

const view = (chip: StripChip): StripView => ({
  groups: [
    { id: "needs_you", icon: "hand", tone: "attn", title: "Needs you · 1", chips: [chip], resume_all: null },
  ],
  empty: null,
  next: null,
});

let host: HTMLElement;
let root: Root;

beforeEach(async () => {
  // The page's own style sheets (the test runner leaves CSS imports empty).
  const fsModule = "node:fs";
  const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
  const here = (import.meta as ImportMeta & { dirname: string }).dirname;
  for (const file of ["./strip.css", "../sheet/actions.css", "../agent_panel/agent_panel.css"]) {
    const style = document.createElement("style");
    style.textContent = fs.readFileSync(`${here}/${file}`, "utf8");
    document.head.append(style);
  }
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  document.head.innerHTML = "";
});

/** Draws the strip with `chip` and opens its popover. */
function openPopover(chip: StripChip, onOpen = vi.fn()): { popover: HTMLElement; onOpen: typeof onOpen } {
  act(() => {
    root.render(createElement(StripBody, { view: view(chip), project: "SHOP", onOpen }));
  });
  act(() => {
    host.querySelector<HTMLButtonElement>(".strip-chip")?.click();
  });
  const popover = host.querySelector<HTMLElement>('.strip-asks[role="dialog"]');
  if (!popover) throw new Error("no popover");
  return { popover, onOpen };
}

const buttonIn = (scope: Element, text: string) =>
  [...scope.querySelectorAll("button")].find((b) => b.textContent.includes(text));

/** 500 px wide, capped at 560 px and never taller than the window; no fixed height, so a short one fits. */
function expectWide(popover: HTMLElement) {
  const style = getComputedStyle(popover);
  expect(style.width).toBe("500px");
  expect(style.boxSizing).toBe("border-box");
  expect(style.maxHeight).toContain("560px");
  expect(style.display).toBe("flex");
  expect(style.flexDirection).toBe("column");
  expect(["", "auto"]).toContain(style.height);
}

/** The question's line and context are one text that can shrink and scroll, before its options do; `outside` stays out
 * of it, unshrunk. */
function expectTextScrollsAlone(popover: HTMLElement, outside: Element[]): HTMLElement {
  const text = popover.querySelector<HTMLElement>(".sheet-answer-text");
  if (!text) throw new Error("no question text");
  expect(getComputedStyle(text).overflowY).toBe("auto");
  expect(Number.parseFloat(getComputedStyle(text).minHeight)).toBe(0);
  // It gives way first, down to its floor (`QuestionText`); the options then scroll (NEW-d1, NEW-d2).
  expect(Number(getComputedStyle(text).flexShrink)).toBeGreaterThanOrEqual(1000);
  const options = popover.querySelector(".sheet-answer-options");
  if (options) {
    expect(text.contains(options)).toBe(false);
    expect(getComputedStyle(options).flexShrink).toBe("1");
    expect(getComputedStyle(options).overflowY).toBe("auto");
  }
  for (const el of outside) {
    expect(text.contains(el)).toBe(false);
    expect(getComputedStyle(el).flexShrink).toBe("0");
  }
  // The context reads at about 70 characters a line.
  const context = text.querySelector(".sheet-answer-context");
  expect(context && getComputedStyle(context).maxWidth).toBe("400px");
  return text;
}

describe("Noyan's long question in the strip", () => {
  it("opens 500 px wide, capped at 560 px, its text scrolling while the options, Send and the panel link stay in view", () => {
    const { popover } = openPopover(noyan(LONG));
    expect(popover.classList.contains("strip-agent-asks")).toBe(true);
    expectWide(popover);
    const options = popover.querySelector(".sheet-answer-options");
    const bar = popover.querySelector(".agent-question-bar");
    const foot = popover.querySelector(".strip-question-foot");
    if (!options || !bar || !foot) throw new Error("no options, Send or link");
    const text = expectTextScrollsAlone(popover, [bar, foot]);
    expect(text.querySelector(".sheet-answer-line")?.textContent).toBe(LONG.line);
    expect(text.querySelector(".sheet-answer-context")?.textContent).toContain("A third agent costs");
    expect(bar.contains(buttonIn(popover, "Send answer") ?? null)).toBe(true);
    expect(foot.textContent).toBe("Open in the agent panel");
  });

  it("Open in the agent panel closes the popover and shows the conversation, its question above the terminal", () => {
    const { popover, onOpen } = openPopover(noyan(LONG));
    act(() => {
      buttonIn(popover, "Open in the agent panel")?.click();
    });
    expect(host.querySelector(".strip-agent-asks")).toBeNull();
    expect(onOpen).toHaveBeenCalledWith(AGENT);
    // Where that lands: the panel open on Noyan's conversation, which draws its open question above the terminal.
    const selection = createSelection({ project: "SHOP", ticket: null, tab: "agent", focusedTerminal: null });
    const routes: StripRoutes = {
      selection,
      focus: createStore<TerminalFocusRequest | null>(null),
      agentPanel: {
        selection,
        layout: createStore<ShellLayout>({ ...INITIAL_LAYOUT, agentOpen: false }),
        shown: createStore<ShownAgents>({}),
        focus: createStore<AgentPanelFocusRequest | null>(null),
      },
      last: createStore<StripTarget | null>(null),
      blur: vi.fn(),
    };
    openTarget(AGENT, routes);
    expect(routes.agentPanel.layout.get().agentOpen).toBe(true);
    expect(routes.agentPanel.shown.get()).toEqual({ SHOP: "pa3" });
  });

  it("a short question's popover fits its content", () => {
    const { popover } = openPopover(
      noyan({ ...LONG, line: "Add an index on orders.created_at?", context: null }),
    );
    expectWide(popover);
    expect(popover.querySelector(".sheet-answer-context")).toBeNull();
    expect(popover.querySelector<HTMLElement>(".sheet-answer-text")?.style.maxHeight).toBe("");
  });
});

describe("a helper's long question in the strip", () => {
  it("opens 500 px wide, capped at 560 px, drawn as the question box, the note and buttons in view", () => {
    const { popover } = openPopover(helper(LONG.line, LONG.context));
    expect(popover.classList.contains("strip-agent-asks")).toBe(false);
    expectWide(popover);
    const note = popover.querySelector(".strip-asks-note");
    const buttons = popover.querySelector(".strip-asks-buttons");
    if (!note || !buttons) throw new Error("no note or buttons");
    const text = expectTextScrollsAlone(popover, [note, buttons]);
    expect(text.querySelector(".sheet-answer-line")?.textContent).toBe(LONG.line);
    expect(text.querySelectorAll(".sheet-answer-context li")).toHaveLength(2);
    expect(getComputedStyle(popover.querySelector(".strip-asks-question") ?? popover).flexShrink).toBe("1");
  });

  it("Answer 40.2… opens the ticket's Overview, where the question reads whole", () => {
    const { popover, onOpen } = openPopover(helper(LONG.line, LONG.context));
    act(() => {
      buttonIn(popover, "Answer 40.2")?.click();
    });
    expect(onOpen).toHaveBeenCalledWith(OVERVIEW);
    expect(host.querySelector('.strip-asks[role="dialog"]')).toBeNull();
  });

  it("a short question's popover fits its content", () => {
    const { popover } = openPopover(helper("Keep the heart filled after sign-out?", null));
    expectWide(popover);
    expect(popover.querySelector(".sheet-answer-line")?.textContent).toBe(
      "Keep the heart filled after sign-out?",
    );
    expect(popover.querySelector(".sheet-answer-context")).toBeNull();
  });
});
