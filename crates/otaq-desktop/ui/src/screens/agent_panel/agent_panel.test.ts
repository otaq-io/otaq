// @vitest-environment happy-dom
// The Project agent panel's own rules (T-0330, T-0458): Enter sends once and shows the new agent, an example fills the
// box, ⌘L opens the panel and focuses the box or the terminal (and closes it from inside), ⇧⌘L shows the empty panel
// without asking the service anything, the close icon and Esc fold it.
import { act, createElement, useImperativeHandle } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, shellLayout } from "../../app/layout";
import { overlays } from "../../app/overlays";
import { registered, registerScreens } from "../../app/registry";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import type { AgentPanelView } from "../../generated/AgentPanelView";
import { request } from "../../tauri";
import type { TerminalHandle, TerminalProps } from "../../terminal/Terminal";
import { agentPanel } from ".";
import { CloseAgentDialog, RecentList } from "./parts";
import { renderToStaticMarkup } from "react-dom/server";
import {
  leaveExpand,
  moreMenu,
  NEW_ITEM,
  newProjectAgent,
  agentPanelFocusRequests,
  sendFirstMessage,
  shownAgents,
  pickStartOn,
  startOn,
  toggleAgentPanel,
} from "./logic";
import { AgentPanelBody } from "./AgentPanel";

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

const button = (icon: AgentPanelView["header"]["more"]["icon"], label: string) => ({
  icon,
  label,
  keys: null,
});

/** The empty agent panel as `agent_panel.rs` words it for pocket-notes. */
const EMPTY: AgentPanelView = {
  agent: null,
  opens_on: null,
  header: {
    icon: "bot",
    label: "Noyan",
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
    line: {
      avatar: null,
      agent: "Claude Code",
      note: "· starts when you send",
      rename: null,
      reads: "Never edits your code",
    },
    title: "Ask Noyan",
    text: "It reads and runs code in pocket-notes but never edits it. It writes tickets to Backlog, proposing two or more first, and you start them.",
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
    pick: null,
  },
  running: null,
  switcher: {
    rows: [],
    recent_title: "RECENTLY ENDED",
    recent: [],
    new_label: "New conversation",
    new_keys: "⇧⌘L",
  },
  menu: [],
  recent: { title: "RECENTLY ENDED · POCKET-NOTES", rows: [], empty: "No ended conversations yet." },
};

/** "Which tickets wait on NOTE-5?" running, its terminal attached. */
const RUNNING: AgentPanelView = {
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
    line: {
      avatar: null,
      agent: "Claude Code",
      note: "· 52k",
      rename: "Rename",
      reads: "Never edits your code",
    },
    created: {
      line: "CREATED 2 · CHANGED 1",
      folded: true,
      show_on_board: { label: "Show on board", agent: "a1" },
      title: "CREATED 2",
      rows: [
        {
          key: "NOTE-11",
          title: "Slow search query",
          place: "Dev",
          line: "Waits for a free agent · next in line",
          icon: "square-plus",
          edited: false,
          start: null,
          delete: null,
        },
        {
          key: "NOTE-12",
          title: "Cache note previews",
          place: "",
          line: null,
          icon: "square-plus",
          edited: false,
          start: "Start",
          delete: "Delete",
        },
      ],
      changed_title: "CHANGED 1",
      changed: [
        {
          key: "NOTE-5",
          title: "Sync conflicts",
          place: "Backlog",
          line: null,
          icon: "pencil",
          edited: true,
          start: null,
          delete: null,
        },
      ],
    },
    session: "s1",
    notice: null,
    question: null,
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

function show(view: AgentPanelView, followFocus = true) {
  act(() => {
    root.render(createElement(AgentPanelBody, { view, project: "NOTE", followFocus }));
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
  // The frame's `agent_panel` slot, which ⌘L looks for to close the panel from inside.
  box.dataset.slot = "agent_panel";
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
  shownAgents.set({});
  agentPanelFocusRequests.set(null);
  shellLayout.set(INITIAL_LAYOUT);
  selection.set(NOTHING_SELECTED);
});

describe("the empty agent panel", () => {
  it("says what it will start, and nothing runs until Send", () => {
    show(EMPTY);
    for (const text of ["Ask Noyan", "Claude Code", "· starts when you send", "Enter starts Claude Code"]) {
      expect(box.textContent).toContain(text);
    }
    expect(textarea().placeholder).toBe("Ask about pocket-notes, or describe work…");
    expect(ask).not.toHaveBeenCalled();
  });

  it("Enter sends once and switches to the new conversation; Shift+Enter is a new line", async () => {
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
    expect(ask).toHaveBeenCalledWith("agent_panel.send", {
      project: "NOTE",
      message: "Which tickets wait on NOTE-5?",
    });
    await act(async () => {
      answer("a1");
      await Promise.resolve();
    });
    expect(shownAgents.get()).toEqual({ NOTE: "a1" });
    expect(agentPanelFocusRequests.get()).toMatchObject({ agent: "a1" });
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

describe("the agent panel's keys", () => {
  it("registers the panel, ⌘L and ⇧⌘L", () => {
    const app = registerScreens({ agentPanel });
    expect(registered(app)).toMatchObject({ slots: ["agent_panel"] });
    expect(app.shortcuts.names()).toEqual(["focus_agent_panel", "new_project_agent"]);
    // No project shown: both keys pass.
    expect(app.shortcuts.run({ action: "focus_agent_panel" })).toBe(false);
    expect(app.shortcuts.run({ action: "new_project_agent" })).toBe(false);
  });

  it("⌘L opens the panel and focuses the box, or the shown agent's terminal; from inside it closes it", () => {
    const app = registerScreens({ agentPanel });
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: false });
    show(EMPTY);
    act(() => {
      app.shortcuts.run({ action: "focus_agent_panel" });
    });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(document.activeElement).toBe(textarea());

    // The keyboard is in the panel: ⌘L folds it, conversation kept.
    act(() => {
      app.shortcuts.run({ action: "focus_agent_panel" });
    });
    expect(shellLayout.get().agentOpen).toBe(false);

    show(RUNNING);
    act(() => {
      app.shortcuts.run({ action: "focus_agent_panel" });
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

    // The board's Esc (`close_sheet`, never sent from inside a terminal) takes it only while expanded.
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: true, agentExpanded: true });
    expect(leaveExpand()).toBe(true);
    expect(shellLayout.get()).toMatchObject({ agentOpen: true, agentExpanded: false });
    expect(leaveExpand()).toBe(false);
  });

  it("the rail's button opens the panel with the keyboard, or closes an open one", () => {
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    expect(toggleAgentPanel()).toBe(true);
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(agentPanelFocusRequests.get()).not.toBeNull();
    expect(toggleAgentPanel()).toBe(true);
    expect(shellLayout.get().agentOpen).toBe(false);
  });

  it("⇧⌘L shows the empty agent panel, focused, without asking the service anything", () => {
    const app = registerScreens({ agentPanel });
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    shownAgents.set({ NOTE: "a1" });
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: false });
    expect(app.shortcuts.run({ action: "new_project_agent" })).toBe(true);
    expect(shownAgents.get()).toEqual({ NOTE: null });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(agentPanelFocusRequests.get()).toMatchObject({ agent: null });
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
      box.querySelector<HTMLButtonElement>(".agent-panel-created-toggle")?.click();
    });
    expect(box.textContent).toContain("NOTE-12");
    expect(box.querySelectorAll(".agent-panel-created-delete")).toHaveLength(1);
    expect(box.querySelector(".fake-terminal")?.getAttribute("aria-label")).toBe(
      "Tickets waiting on NOTE-5 · Claude Code",
    );
    // The panel's column (`.agent-panel` in agent_panel.css) holds the terminal's well, which fills the height.
    expect(
      box.querySelector("section.agent-panel > .agent-panel-well > .agent-panel-terminal"),
    ).not.toBeNull();
  });

  it("lists the created tickets on top: Start on one in Backlog starts it as a Backlog card's Start does", async () => {
    ask.mockResolvedValue({ outcome: "moved" });
    const running = RUNNING.running;
    const created = running?.created;
    if (!running || !created) throw new Error("the fixture runs and created tickets");
    show({ ...RUNNING, running: { ...running, created: { ...created, folded: false } } });
    expect(box.textContent).toContain("CREATED 2");
    expect(box.textContent).not.toContain("CREATED 2 · CHANGED 1");
    // The started one says where it is and why it waits; the one in Backlog has Start (Noyan never starts it).
    const rows = [...box.querySelectorAll(".agent-panel-created-row")];
    expect(rows.map((r) => r.querySelector(".agent-panel-created-key")?.textContent)).toEqual([
      "NOTE-11",
      "NOTE-12",
      "NOTE-5",
    ]);
    expect(rows[0]?.textContent).toContain("Dev");
    expect(rows[0]?.textContent).toContain("Waits for a free agent · next in line");
    expect(rows[0]?.textContent).not.toContain("Start");
    expect(rows[2]?.textContent).not.toContain("Start");
    expect(box.textContent).toContain("CHANGED 1");
    const start = [...(rows[1]?.querySelectorAll("button") ?? [])].find((b) => b.textContent === "Start");
    await act(async () => {
      start?.click();
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("board.start", { key: "NOTE-12" });
  });

  it("the switcher continues a conversation: Resume on an ended one, and the recently ended ones", async () => {
    ask.mockResolvedValue("a9");
    show({
      ...RUNNING,
      switcher: {
        ...RUNNING.switcher,
        rows: [
          {
            id: "a1",
            name: "Tickets waiting on NOTE-5",
            unnamed: false,
            pill: { palette: "attn", icon: "hand", label: "Waiting for you" },
            avatar: null,
            selected: true,
            resume: null,
          },
          {
            id: "a2",
            name: "Footer links",
            unnamed: false,
            pill: { palette: "queued", icon: "circle-stop", label: "Ended" },
            avatar: null,
            selected: false,
            resume: "Resume",
          },
        ],
        recent: [
          {
            id: "a9",
            name: "Footer audit",
            unnamed: false,
            pill: { palette: "queued", icon: "circle-stop", label: "Ended by restart" },
            avatar: null,
            selected: false,
            resume: "Resume",
          },
        ],
      },
    });
    act(() => {
      box.querySelector<HTMLButtonElement>(".agent-panel-switch")?.click();
    });
    const switcher = document.querySelector(".agent-panel-switcher");
    expect(switcher?.textContent).toContain("RECENTLY ENDED");
    expect(switcher?.textContent).toContain("Footer audit");
    const resumes = [
      ...(switcher?.querySelectorAll<HTMLButtonElement>(".agent-panel-switcher-resume") ?? []),
    ];
    expect(resumes).toHaveLength(2);
    await act(async () => {
      resumes[1]?.click();
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("agent_panel.resume", { id: "a9" });
    expect(shownAgents.get().NOTE).toBe("a9");
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
          empty: "No ended conversations yet.",
        },
        onResume: () => undefined,
      }),
    );
    expect(recent).toContain("Footer audit");
    expect(recent).not.toContain("No ended conversations yet.");
  });
});

/** *18 `VDJkG`*: the agent on Standard · Codex, Codex logged out: it waits, and no other agent is offered. */
const CODEX_OUT: NonNullable<AgentPanelView["running"]>["notice"] = {
  role: null,
  tone: "amber",
  avatar: null,
  heading: "Codex isn't available · signed out. This agent continues after you sign in.",
  text: null,
  model: null,
  buttons: [{ label: "Sign in…", action: { kind: "sign_in", tool: "codex" }, primary: true }],
};

const click = (label: string) => {
  const found = [...box.querySelectorAll("button")].find((b) => b.textContent === label);
  if (!found) throw new Error(`no ${label}`);
  act(() => {
    found.click();
  });
};

describe("Noyan's question (OTQ-9, d0119)", () => {
  const QUESTION = {
    project: "NOTE",
    agent: "a1",
    title: "Noyan · Tickets waiting on NOTE-5 asks",
    answer: {
      question: "q7",
      text: "Split NOTE-5 into two tickets?",
      rows: [
        {
          number: 1,
          label: "Split it",
          recommended: false,
          picked: false,
          other: false,
          note: "Two smaller reviews.",
        },
        {
          number: 2,
          label: "Keep one",
          recommended: true,
          picked: true,
          other: false,
          note: "One branch, one review.",
        },
        {
          number: 3,
          label: "Other… type your own answer",
          recommended: false,
          picked: false,
          other: true,
          note: null,
        },
      ],
      recommended_label: "Recommended",
      other_placeholder: "Type your answer",
      hint: "1–4 pick · ↵ sends",
      in_terminal: "Answer in terminal",
      send: "Send answer",
    },
  };
  const asking = (): AgentPanelView => ({
    ...RUNNING,
    running: RUNNING.running && { ...RUNNING.running, question: QUESTION },
  });

  afterEach(() => {
    overlays.store.set([]);
  });

  it("shows above the terminal with its options; Answer in terminal focuses the terminal", () => {
    show(asking());
    const question = box.querySelector<HTMLElement>(".agent-panel-question");
    expect(question?.textContent).toContain("Split NOTE-5 into two tickets?");
    expect(question?.textContent).toContain("One branch, one review.");
    expect(question?.querySelector('[aria-checked="true"]')?.textContent).toContain("Keep one");
    // It sits before the terminal.
    const terminal = box.querySelector(".fake-terminal");
    expect(terminal && question?.compareDocumentPosition(terminal)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    terminalFocus.mockReset();
    click("Answer in terminal");
    expect(terminalFocus).toHaveBeenCalled();
    expect(ask).not.toHaveBeenCalled();
  });

  it("Send answer sends the pick; unsent text in its terminal asks Keep it and cancel / Clear it and send", async () => {
    show(asking());
    ask.mockResolvedValueOnce({
      outcome: "ask",
      title: "Noyan · Tickets waiting on NOTE-5 has unsent text in its terminal",
      body: "Clear it and send clears the typed text, then sends. Keep it and cancel leaves the terminal as it is; nothing is sent. Your answer: option 2.",
      clear: "Clear it and send",
      cancel: "Cancel",
    });
    await act(async () => {
      [...box.querySelectorAll("button")].find((b) => b.textContent.includes("Send answer"))?.click();
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("agent_panel.answer", {
      project: "NOTE",
      agent: "a1",
      question: "q7",
      pick: 2,
      other: "",
      clear: false,
    });
    const [confirm] = overlays.store.get();
    const params = confirm?.params as { title: string; confirmLabel: string; onConfirm: () => unknown };
    expect(confirm?.id).toBe("confirm");
    expect(params.title).toBe("Noyan · Tickets waiting on NOTE-5 has unsent text in its terminal");
    expect(params.confirmLabel).toBe("Clear it and send");
    ask.mockResolvedValueOnce({ outcome: "done" });
    await act(async () => {
      await params.onConfirm();
    });
    expect(ask).toHaveBeenLastCalledWith("agent_panel.answer", {
      project: "NOTE",
      agent: "a1",
      question: "q7",
      pick: 2,
      other: "",
      clear: true,
    });
  });

  it("an ended conversation shows no question", () => {
    const { running } = asking();
    show({ ...asking(), running: running && { ...running, ended: true } });
    expect(box.querySelector(".agent-panel-question")).toBeNull();
  });
});

describe("a tool problem's notice (M23)", () => {
  afterEach(() => {
    startOn.set({});
  });

  it("shows above the terminal, amber; Sign in… opens the tool's login", () => {
    ask.mockResolvedValue({ session: "s9", tab: "t9", title: "Codex sign in" });
    show({ ...RUNNING, running: RUNNING.running && { ...RUNNING.running, notice: CODEX_OUT } });
    const notice = box.querySelector(".agent-panel-notice .tool-problem");
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
          ],
        },
      },
    });
    expect(box.querySelector(".agent-panel-notice .tool-problem")?.getAttribute("data-tone")).toBe("neutral");
    click("Resume after reset");
    expect(ask).toHaveBeenCalledWith("limits.resume_tool", { tool: "codex" });
  });

  it("the notice offers no other agent: the conversation waits for its own tool", () => {
    show({ ...RUNNING, running: RUNNING.running && { ...RUNNING.running, notice: CODEX_OUT } });
    const labels = [...box.querySelectorAll(".agent-panel-notice button")].map((b) => b.textContent);
    expect(labels).toEqual(["Sign in…"]);
  });

  it("a new conversation picks its own agent, and its first message starts it there", async () => {
    show({
      ...EMPTY,
      empty: EMPTY.empty && {
        ...EMPTY.empty,
        pick: {
          label: "Agent",
          usual: "noyan",
          options: [
            { id: "noyan", label: "Noyan", selected: true },
            { id: "sonnet", label: "Sonnet", selected: false },
          ],
        },
      },
    });
    expect(box.querySelector('.agent-panel-input-row button[aria-label="Agent"]')?.textContent).toContain(
      "Noyan",
    );
    pickStartOn("NOTE", "sonnet", "noyan");
    expect(ask).not.toHaveBeenCalled();
    expect(startOn.get()).toEqual({ NOTE: "sonnet" });
    // Picking the agent Noyan uses again drops the pick.
    pickStartOn("NOTE", "noyan", "noyan");
    expect(startOn.get()).toEqual({});
    pickStartOn("NOTE", "sonnet", "noyan");

    ask.mockResolvedValue("a2");
    show({ ...EMPTY, empty: EMPTY.empty && { ...EMPTY.empty, starts_on: "sonnet" } });
    type("Why is checkout slow?");
    await act(async () => {
      enter();
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("agent_panel.send", {
      project: "NOTE",
      message: "Why is checkout slow?",
      agent: "sonnet",
    });
    expect(startOn.get()).toEqual({});
    expect(shownAgents.get()).toEqual({ NOTE: "a2" });
  });

  it("+ drops the choice: the empty agent panel starts on the agent Noyan uses again", () => {
    pickStartOn("NOTE", "sonnet", "noyan");
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
    expect(box.querySelector(".agent-panel-notice")?.textContent).toContain("Model unavailable");
    expect(box.querySelector("textarea")).not.toBeNull();
  });
});

describe("a narrow panel's header", () => {
  const MENU: AgentPanelView["menu"] = [
    {
      id: "rename",
      label: "Rename",
      icon: "pencil",
      shortcut: null,
      danger: false,
      disabled: false,
      hint: null,
      separated: false,
      checked: false,
      header: false,
    },
    {
      id: "recent",
      label: "Recently ended…",
      icon: "history",
      shortcut: null,
      danger: false,
      disabled: false,
      hint: null,
      separated: true,
      checked: false,
      header: false,
    },
  ];

  it("folds + into the … menu, worded as the header's button, only while a conversation is shown", () => {
    const newButton = RUNNING.header.new;
    expect(moreMenu(MENU, newButton, false, true)).toBe(MENU);
    expect(moreMenu(MENU, newButton, true, false)).toBe(MENU);
    const folded = moreMenu(MENU, newButton, true, true);
    expect(folded.map((item) => [item.id, item.label, item.shortcut, item.separated])).toEqual([
      [NEW_ITEM, "New conversation", "⇧⌘L", false],
      ["rename", "Rename", null, true],
      ["recent", "Recently ended…", null, true],
    ]);
  });

  it("… opens with New conversation when the header folded its buttons, and choosing it shows the empty panel", () => {
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    shownAgents.set({ NOTE: "a1" });
    show({ ...RUNNING, menu: MENU }, false);
    const more = () => box.querySelector<HTMLButtonElement>('button[aria-label="More"]');
    const items = () => [...document.querySelectorAll(".menu-item-label")].map((label) => label.textContent);
    act(() => {
      more()?.click();
    });
    expect(items()).toEqual(["Rename", "Recently ended…"]);
    act(() => {
      more()?.click();
    });
    // The container query hides Recently ended and +: their buttons have no boxes.
    const folded = box.querySelectorAll<HTMLButtonElement>("button[data-folds]");
    expect([...folded].map((b) => b.getAttribute("aria-label"))).toEqual([
      "Recently ended",
      "New conversation",
    ]);
    for (const b of folded) vi.spyOn(b, "getClientRects").mockReturnValue([] as unknown as DOMRectList);
    act(() => {
      more()?.click();
    });
    expect(items()).toEqual(["New conversation", "Rename", "Recently ended…"]);
    act(() => {
      [...document.querySelectorAll<HTMLButtonElement>(".menu-item")]
        .find((item) => item.textContent.includes("New conversation"))
        ?.click();
    });
    expect(shownAgents.get()).toEqual({ NOTE: null });
  });
});
