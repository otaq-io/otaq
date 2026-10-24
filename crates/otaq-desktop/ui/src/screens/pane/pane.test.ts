// @vitest-environment happy-dom
// The Project agent panel's own rules (T-0330, T-0458): Enter sends once and shows the new agent, an example fills the
// box, ⌘L opens the panel and focuses the box or the terminal (and closes it from inside), ⇧⌘L shows the empty panel
// without asking the service anything, the close icon and Esc fold it.
import { act, createElement, useImperativeHandle } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, shellLayout } from "../../app/layout";
import { registered, registerScreens } from "../../app/registry";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import type { PaneView } from "../../generated/PaneView";
import { request } from "../../tauri";
import type { TerminalHandle, TerminalProps } from "../../terminal/Terminal";
import { pane } from ".";
import { CloseAgentDialog, RecentList } from "./parts";
import { renderToStaticMarkup } from "react-dom/server";
import {
  leaveExpand,
  newProjectAgent,
  paneFocusRequests,
  sendFirstMessage,
  shownAgents,
  startNewAgentOn,
  startOn,
  toggleAgentPanel,
} from "./logic";
import { PaneBody } from "./Pane";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

// The real terminal needs a canvas; this one only takes the keyboard.
const terminalFocus = vi.fn();
vi.mock("../../terminal/Terminal", () => ({
  Terminal: ({ label, ref }: TerminalProps) => {
    useImperativeHandle(ref, () => ({ focus: terminalFocus }) as unknown as TerminalHandle);
    return createElement("div", { className: "fake-terminal", "aria-label": label });
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ask = vi.mocked(request);

const button = (icon: PaneView["header"]["more"]["icon"], label: string) => ({ icon, label, keys: null });

/** The empty pane as `agent_pane.rs` words it for pocket-notes. */
const EMPTY: PaneView = {
  agent: null,
  opens_on: null,
  header: {
    icon: "bot",
    label: "Project agent",
    title: "New conversation",
    unnamed: false,
    switcher: false,
    pill: null,
    recent: button("history", "Recently ended"),
    new: { icon: "plus", label: "New conversation", keys: "⇧⌘L" },
    more: button("ellipsis", "More"),
    expand: button("maximize-2", "Expand"),
    collapse: { icon: "minimize-2", label: "Collapse", keys: "Esc" },
    close: { icon: "panel-left-close", label: "Close", keys: "⌘L" },
  },
  empty: {
    line: { avatar: null, agent: "Claude Code", note: "· starts when you send", rename: null },
    title: "Ask the Project agent",
    text: "It reads and runs code in pocket-notes (never edits) and creates or changes Backlog tickets.",
    examples: [
      { label: "Turn this into tickets: …", fill: "Turn this into tickets: " },
      { label: "Why is a ticket stuck?", fill: "Why is a ticket stuck?" },
    ],
    recent_title: "RECENTLY ENDED",
    placeholder: "Ask about pocket-notes, or describe work…",
    hint: "Enter starts Claude Code",
    send: "Send",
    notice: null,
    starts_on: null,
  },
  running: null,
  switcher: { rows: [], new_label: "New conversation", new_keys: "⇧⌘L" },
  menu: [],
  recent: { title: "RECENTLY ENDED · POCKET-NOTES", rows: [], empty: "No ended Project agents yet." },
};

/** "Which tickets wait on NOTE-5?" running, its terminal attached. */
const RUNNING: PaneView = {
  ...EMPTY,
  agent: "a1",
  header: {
    ...EMPTY.header,
    title: "Tickets waiting on NOTE-5",
    switcher: true,
    pill: { palette: "attn", icon: "hand", label: "Waiting for you" },
  },
  empty: null,
  running: {
    line: { avatar: null, agent: "Claude Code", note: "· 52k", rename: "Rename" },
    created: {
      line: "CREATED 2 · CHANGED 1",
      folded: true,
      show_on_board: { label: "Show on board", agent: "a1" },
      rows: [
        {
          key: "NOTE-12",
          title: "Cache note previews",
          place: "Backlog",
          icon: "square-plus",
          edited: false,
          delete: "Delete",
        },
      ],
    },
    session: "s1",
    notice: null,
    ended: false,
    footer: {
      text: "Typing goes to the agent",
      keys: "⌘L",
      keys_label: "focus",
      resume: null,
      resume_note: null,
    },
  },
};

let root: Root;
let box: HTMLElement;

function show(view: PaneView, followFocus = true) {
  act(() => {
    root.render(createElement(PaneBody, { view, project: "NOTE", followFocus }));
  });
}

const textarea = () => {
  const found = box.querySelector("textarea");
  if (!found) throw new Error("no box");
  return found;
};

function type(text: string) {
  act(() => {
    Reflect.set(HTMLTextAreaElement.prototype, "value", text, textarea());
    textarea().dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function enter(shiftKey = false) {
  act(() => {
    textarea().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", shiftKey, bubbles: true }));
  });
}

beforeEach(() => {
  ask.mockReset();
  terminalFocus.mockReset();
  box = document.body.appendChild(document.createElement("div"));
  // The frame's `pane` slot, which ⌘L looks for to close the panel from inside.
  box.dataset.slot = "agent_panel";
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
  shownAgents.set({});
  paneFocusRequests.set(null);
  shellLayout.set(INITIAL_LAYOUT);
  selection.set(NOTHING_SELECTED);
});

describe("the empty pane", () => {
  it("says what it will start, and nothing runs until Send", () => {
    show(EMPTY);
    for (const text of [
      "Ask the Project agent",
      "Claude Code",
      "· starts when you send",
      "Enter starts Claude Code",
    ]) {
      expect(box.textContent).toContain(text);
    }
    expect(textarea().placeholder).toBe("Ask about pocket-notes, or describe work…");
    expect(ask).not.toHaveBeenCalled();
  });

  it("Enter sends once and switches to the new agent; Shift+Enter is a new line", async () => {
    let answer: (id: string) => void = () => undefined;
    ask.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    show(EMPTY);
    enter();
    expect(ask).not.toHaveBeenCalled();
    type("Which tickets wait on NOTE-5?");
    enter(true);
    expect(ask).not.toHaveBeenCalled();
    enter();
    enter();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask).toHaveBeenCalledWith("pane.send", {
      project: "NOTE",
      message: "Which tickets wait on NOTE-5?",
    });
    await act(async () => {
      answer("a1");
      await Promise.resolve();
    });
    expect(shownAgents.get()).toEqual({ NOTE: "a1" });
    expect(paneFocusRequests.get()).toMatchObject({ agent: "a1" });
    expect(textarea().value).toBe("");
    // The new agent's terminal shows: it is asked for the keyboard at once, which it takes as soon as it opens
    // (`Terminal.focus.test.ts`), so what the user goes on typing reaches the agent (d0126).
    expect(terminalFocus).not.toHaveBeenCalled();
    show(RUNNING);
    expect(terminalFocus).toHaveBeenCalledTimes(1);
  });

  it("a second Send for the same project waits for the first", async () => {
    let answer: (id: string) => void = () => undefined;
    ask.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    const first = sendFirstMessage("NOTE", "hello", request);
    expect(await sendFirstMessage("NOTE", "hello", request)).toBeNull();
    answer("a7");
    expect(await first).toBe("a7");
    expect(ask).toHaveBeenCalledTimes(1);
  });

  it("an example click fills the box and never sends", () => {
    show(EMPTY);
    const example = [...box.querySelectorAll("button")].find(
      (b) => b.textContent === "Turn this into tickets: …",
    );
    act(() => {
      example?.click();
    });
    expect(textarea().value).toBe("Turn this into tickets: ");
    expect(ask).not.toHaveBeenCalled();
  });
});

describe("the pane's keys", () => {
  it("registers the panel, ⌘L and ⇧⌘L", () => {
    const app = registerScreens({ pane });
    expect(registered(app)).toMatchObject({ slots: ["agent_panel"] });
    expect(app.shortcuts.names()).toEqual(["focus_pane", "new_project_agent"]);
    // No project shown: both keys pass.
    expect(app.shortcuts.run({ action: "focus_pane" })).toBe(false);
    expect(app.shortcuts.run({ action: "new_project_agent" })).toBe(false);
  });

  it("⌘L opens the panel and focuses the box, or the shown agent's terminal; from inside it closes it", () => {
    const app = registerScreens({ pane });
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: false });
    show(EMPTY);
    act(() => {
      app.shortcuts.run({ action: "focus_pane" });
    });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(document.activeElement).toBe(textarea());

    // The keyboard is in the panel: ⌘L folds it, conversation kept.
    act(() => {
      app.shortcuts.run({ action: "focus_pane" });
    });
    expect(shellLayout.get().agentOpen).toBe(false);

    show(RUNNING);
    act(() => {
      app.shortcuts.run({ action: "focus_pane" });
    });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(terminalFocus).toHaveBeenCalledTimes(1);
    expect(ask).not.toHaveBeenCalled();
  });

  it("the close icon and Esc in the box fold the panel; the box keeps its words", () => {
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: true });
    show(EMPTY);
    act(() => {
      box.querySelector<HTMLButtonElement>('button[aria-label="Close"]')?.click();
    });
    expect(shellLayout.get().agentOpen).toBe(false);
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: true });
    type("half a question");
    act(() => {
      textarea().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(shellLayout.get().agentOpen).toBe(false);
    expect(textarea().value).toBe("half a question");
  });

  // T-0502 (dogfood #7).
  it("Expand lays the open panel over the board; Collapse, Esc in the box or Esc outside a terminal bring it back", () => {
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: true });
    show(EMPTY);
    const header = (label: string) => box.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
    act(() => {
      header("Expand")?.click();
    });
    expect(shellLayout.get()).toMatchObject({ agentOpen: true, agentExpanded: true });
    expect(header("Expand")).toBeNull();
    act(() => {
      header("Collapse")?.click();
    });
    expect(shellLayout.get()).toMatchObject({ agentOpen: true, agentExpanded: false });

    // Esc in the box: first back beside the board, then folded.
    act(() => {
      header("Expand")?.click();
    });
    const esc = () => {
      act(() => {
        textarea().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      });
    };
    esc();
    expect(shellLayout.get()).toMatchObject({ agentOpen: true, agentExpanded: false });
    esc();
    expect(shellLayout.get().agentOpen).toBe(false);

    // The board's Esc (`close_panel`, never sent from inside a terminal) takes it only while expanded.
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: true, agentExpanded: true });
    expect(leaveExpand()).toBe(true);
    expect(shellLayout.get()).toMatchObject({ agentOpen: true, agentExpanded: false });
    expect(leaveExpand()).toBe(false);
  });

  it("the rail's button opens the panel with the keyboard, or closes an open one", () => {
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    expect(toggleAgentPanel()).toBe(true);
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(paneFocusRequests.get()).not.toBeNull();
    expect(toggleAgentPanel()).toBe(true);
    expect(shellLayout.get().agentOpen).toBe(false);
  });

  it("⇧⌘L shows the empty pane, focused, without asking the service anything", () => {
    const app = registerScreens({ pane });
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    shownAgents.set({ NOTE: "a1" });
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: false });
    expect(app.shortcuts.run({ action: "new_project_agent" })).toBe(true);
    expect(shownAgents.get()).toEqual({ NOTE: null });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(paneFocusRequests.get()).toMatchObject({ agent: null });
    expect(ask).not.toHaveBeenCalled();
  });
});

describe("a running agent", () => {
  it("draws the header, the agent line, Created and changed folded, the terminal and the footer", () => {
    show(RUNNING, false);
    for (const text of [
      "Tickets waiting on NOTE-5",
      "Waiting for you",
      "· 52k",
      "Rename",
      "CREATED 2 · CHANGED 1",
      "Show on board",
      "Typing goes to the agent",
      "⌘L",
    ]) {
      expect(box.textContent).toContain(text);
    }
    // Folded at the 1280 class: the rows open on a click.
    expect(box.textContent).not.toContain("NOTE-12");
    act(() => {
      box.querySelector<HTMLButtonElement>(".pane-created-toggle")?.click();
    });
    expect(box.textContent).toContain("NOTE-12");
    expect(box.querySelectorAll(".pane-created-delete")).toHaveLength(1);
    expect(box.querySelector(".fake-terminal")?.getAttribute("aria-label")).toBe(
      "Tickets waiting on NOTE-5 · Claude Code",
    );
  });

  it("asks before ending mid-turn, and lists Recently ended", () => {
    const close = renderToStaticMarkup(
      createElement(CloseAgentDialog, {
        dialog: {
          title: "End “Checkout slowness” while it’s working?",
          mid_turn: "It’s mid-turn. Ending stops it now.",
          text: "You can resume the conversation from Recently ended.",
          confirm: "Stop and end",
        },
        onConfirm: () => undefined,
        onCancel: () => undefined,
      }),
    );
    expect(close).toContain("Stop and end");
    expect(close).toContain("mid-turn");
    const recent = renderToStaticMarkup(
      createElement(RecentList, {
        recent: {
          title: "RECENTLY ENDED · POCKET-NOTES",
          rows: [
            {
              id: "a9",
              name: "Footer audit",
              when: "yesterday · ended normally",
              ended: "yesterday",
              resume: "Resume",
            },
          ],
          empty: "No ended Project agents yet.",
        },
        onResume: () => undefined,
      }),
    );
    expect(recent).toContain("Footer audit");
    expect(recent).not.toContain("No ended Project agents yet.");
  });
});

/** *18 `VDJkG`*: the agent on Standard · Codex, Codex logged out; Sonnet is Standard's fallback. */
const CODEX_OUT: NonNullable<PaneView["running"]>["notice"] = {
  role: null,
  tone: "amber",
  avatar: null,
  heading: "Codex isn't available · signed out. This agent continues after you sign in.",
  text: null,
  model: null,
  buttons: [
    { label: "Sign in…", action: { kind: "sign_in", tool: "codex" }, primary: true },
    { label: "New agent on Sonnet", action: { kind: "new_agent", agent: "sonnet" }, primary: false },
  ],
};

const click = (label: string) => {
  const found = [...box.querySelectorAll("button")].find((b) => b.textContent === label);
  if (!found) throw new Error(`no ${label}`);
  act(() => {
    found.click();
  });
};

describe("a tool problem's notice (M23)", () => {
  afterEach(() => {
    startOn.set({});
  });

  it("shows above the terminal, amber; Sign in… opens the tool's login", () => {
    ask.mockResolvedValue({ session: "s9", tab: "t9", title: "Codex sign in" });
    show({ ...RUNNING, running: RUNNING.running && { ...RUNNING.running, notice: CODEX_OUT } });
    const notice = box.querySelector(".pane-notice .tool-problem");
    expect(notice?.getAttribute("data-tone")).toBe("amber");
    expect(notice?.textContent).toContain("This agent continues after you sign in.");
    click("Sign in…");
    expect(ask).toHaveBeenCalledWith("agent_tools.sign_in", { tool: "codex" });
  });

  it("Codex at its limit: a neutral notice whose Resume after reset resumes Codex", () => {
    ask.mockResolvedValue(null);
    show({
      ...RUNNING,
      running: RUNNING.running && {
        ...RUNNING.running,
        notice: {
          ...CODEX_OUT,
          tone: "neutral",
          heading: "Codex limit reached · resets 5:10 pm. This agent continues after the reset.",
          buttons: [
            { label: "Resume after reset", action: { kind: "resume_tool", tool: "codex" }, primary: true },
            { label: "New agent on Sonnet", action: { kind: "new_agent", agent: "sonnet" }, primary: false },
          ],
        },
      },
    });
    expect(box.querySelector(".pane-notice .tool-problem")?.getAttribute("data-tone")).toBe("neutral");
    click("Resume after reset");
    expect(ask).toHaveBeenCalledWith("limits.resume_tool", { tool: "codex" });
  });

  it("New agent on Sonnet opens the empty pane on Sonnet, and its first message starts it there", async () => {
    show({ ...RUNNING, running: RUNNING.running && { ...RUNNING.running, notice: CODEX_OUT } });
    click("New agent on Sonnet");
    expect(ask).not.toHaveBeenCalled();
    expect(shownAgents.get()).toEqual({ NOTE: null });
    expect(startOn.get()).toEqual({ NOTE: "sonnet" });
    expect(paneFocusRequests.get()).toMatchObject({ agent: null });

    ask.mockResolvedValue("a2");
    show({ ...EMPTY, empty: EMPTY.empty && { ...EMPTY.empty, starts_on: "sonnet" } });
    type("Why is checkout slow?");
    await act(async () => {
      enter();
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("pane.send", {
      project: "NOTE",
      message: "Why is checkout slow?",
      agent: "sonnet",
    });
    expect(startOn.get()).toEqual({});
    expect(shownAgents.get()).toEqual({ NOTE: "a2" });
  });

  it("+ drops the choice: the empty pane starts on the Project agent default again", () => {
    startNewAgentOn("NOTE", "sonnet");
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    newProjectAgent();
    expect(startOn.get()).toEqual({});
  });

  it("a held first message shows its notice above the box", () => {
    show({
      ...EMPTY,
      empty: EMPTY.empty && {
        ...EMPTY.empty,
        notice: {
          ...CODEX_OUT,
          role: "project_agent",
          heading: "Model unavailable",
          text: "Codex no longer lists gpt-5.4, the model of Old GPT.",
          model: "gpt-5.4",
          buttons: [
            {
              label: "Change agent",
              action: { kind: "agent_settings", project: "NOTE", agent: "old-gpt" },
              primary: true,
            },
          ],
        },
      },
    });
    expect(box.querySelector(".pane-notice")?.textContent).toContain("Model unavailable");
    expect(box.querySelector("textarea")).not.toBeNull();
  });
});
