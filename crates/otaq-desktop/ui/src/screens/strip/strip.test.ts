// @vitest-environment happy-dom
// The Needs you strip's own rules (T-0459; NYS-01…NYS-03, d0114, d0016): it draws Rust's groups and chips with Next at
// its end; a chip opens its ticket, its epic's Lead tab or the Project agent panel; Next asks from what the page shows
// and opens the answer, switching project; nothing opens by itself; a suggestion's Not now and Confirm; Resume all
// asks for the strip's project.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, type ShellLayout } from "../../app/layout";
import { overlays } from "../../app/overlays";
import { registered, registerScreens } from "../../app/registry";
import { createSelection, type TerminalFocusRequest } from "../../app/selection";
import { createStore } from "../../app/store";
import type { SheetContent } from "../../generated/SheetContent";
import type { StripTarget } from "../../generated/StripTarget";
import type { StripView } from "../../generated/StripView";
import { request } from "../../tauri";
import type { PaneFocusRequest, ShownAgents } from "../pane/logic";
import { strip } from ".";
import { confirmSuggestion, dismissSuggestion, openOf, openTarget, runNext, type StripRoutes } from "./logic";
import { resumeAll, ResumeAllBody } from "./ResumeAll";
import { StripBody } from "./Strip";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ask = vi.mocked(request);

const ticket = (key: string, tab: SheetContent = "agent", project = "SHOP") =>
  ({ kind: "ticket", project, key, tab }) as const;

/** `needs_you_strip.rs`'s words for shop-web with every group (`s24-12`). */
const VIEW: StripView = {
  groups: [
    {
      id: "needs_you",
      icon: "hand",
      tone: "attn",
      title: "Needs you · 3",
      chips: [
        {
          label: "SHOP-22",
          reason: "allow a command",
          icon: "hand",
          tone: "attn",
          title: "SHOP-22 Fix flaky login redirect · permission: allow rm -rf .next",
          target: ticket("SHOP-22"),
        },
        {
          label: "40.4",
          reason: "via the lead",
          icon: "message-circle-question-mark",
          tone: "attn",
          title: "SHOP-40.4 Share link · asks",
          target: ticket("SHOP-40.4"),
        },
        {
          label: "Agent",
          reason: "a question",
          icon: "bot",
          tone: "investigate",
          title: "Project agent “Checkout slowness” · asks",
          target: { kind: "agent", project: "SHOP", agent: "pa3" },
        },
      ],
      suggestions: [],
      resume_all: null,
    },
    {
      id: "suggestions",
      icon: "lightbulb",
      tone: "investigate",
      title: "Suggestion",
      chips: [],
      suggestions: [
        {
          id: "sg1",
          ticket: "SHOP-40.6",
          label: "Close 40.6?",
          heading: "Wishlist lead suggests",
          text: "PAY-123 doesn't ask for an export.",
          not_now: "Not now",
          confirm: "Confirm",
          dialog: "close",
          line: "Suggested by Wishlist lead: “PAY-123 doesn't ask for an export.”",
        },
      ],
      resume_all: null,
    },
    {
      id: "interrupted",
      icon: "circle-pause",
      tone: "paused",
      title: "Interrupted · 3",
      chips: [],
      suggestions: [],
      resume_all: "Resume all",
    },
    {
      id: "checks",
      icon: "user-check",
      tone: "review",
      title: "Checks · 1",
      chips: [
        {
          label: "SHOP-18",
          reason: "your check",
          icon: "user-check",
          tone: "review",
          title: "SHOP-18 Checkout API · your check",
          target: ticket("SHOP-18", "files"),
        },
      ],
      suggestions: [],
      resume_all: null,
    },
  ],
  empty: null,
  next: { label: "Next", icon: "arrow-right", keys: "⌘J", elsewhere: false },
};

const EMPTY: StripView = {
  groups: [],
  empty: { icon: "circle-check", title: "Needs you · 0 here", text: "You’re through shop-web’s items." },
  next: { label: "Next · billing-api, 1", icon: "arrow-right", keys: "⌘J", elsewhere: true },
};

function routes(project: string | null = "SHOP"): StripRoutes {
  const selection = createSelection({ project, ticket: null, tab: "agent", focusedTerminal: null });
  return {
    selection,
    focus: createStore<TerminalFocusRequest | null>(null),
    pane: {
      selection,
      layout: createStore<ShellLayout>({ ...INITIAL_LAYOUT, agentOpen: false }),
      shown: createStore<ShownAgents>({}),
      focus: createStore<PaneFocusRequest | null>(null),
    },
    last: createStore<StripTarget | null>(null),
    blur: vi.fn(),
  };
}

let root: Root;
let box: HTMLElement;

beforeEach(() => {
  ask.mockReset();
  overlays.store.set([]);
  box = document.createElement("div");
  document.body.append(box);
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
});

function show(view: StripView, onOpen = vi.fn(), onNext = vi.fn()) {
  act(() => {
    root.render(createElement(StripBody, { view, project: "SHOP", onOpen, onNext }));
  });
  return { onOpen, onNext };
}

function button(text: string): HTMLButtonElement {
  const found = [...box.querySelectorAll("button")].find((b) => b.textContent.includes(text));
  if (!found) throw new Error(`no button ${text}`);
  return found;
}

function click(text: string) {
  act(() => {
    button(text).click();
  });
}

describe("the strip's markup", () => {
  it("registers Resume all's confirmation and ⌘J", () => {
    const r = registered(registerScreens({ strip }));
    expect(r.overlays).toEqual(["resume_all"]);
    expect(r.shortcuts).toEqual(["next_inbox_item"]);
  });

  it("draws every group's head and chips in order, with dividers, Resume all and Next with its key", () => {
    const html = renderToStaticMarkup(createElement(StripBody, { view: VIEW, project: "SHOP" }));
    const texts = [
      "Needs you · 3",
      "SHOP-22",
      "allow a command",
      "40.4",
      "via the lead",
      "Agent",
      "Suggestion",
      "Close 40.6?",
      "Interrupted · 3",
      "Resume all",
      "Checks · 1",
      "SHOP-18",
      "your check",
      "Next",
      "⌘J",
    ];
    const at = texts.map((t) => html.indexOf(t));
    expect(at.every((n) => n >= 0)).toBe(true);
    expect(at).toEqual(at.toSorted((a, b) => a - b));
    expect(html.match(/class="strip-divider"/g)).toHaveLength(3);
    expect(html).toContain('class="strip-chip" data-tone="attn"');
    expect(html).toContain('class="strip-head" data-tone="review"');
    expect(html).not.toContain("strip-empty");
    expect(html).not.toContain("data-elsewhere");
  });

  it("draws “Needs you · 0 here” and the accent Next to another project", () => {
    const html = renderToStaticMarkup(createElement(StripBody, { view: EMPTY, project: "SHOP" }));
    expect(html).toContain("Needs you · 0 here");
    expect(html).toContain("You’re through shop-web’s items.");
    expect(html).toContain('data-elsewhere="true"');
    expect(html).toContain("Next · billing-api, 1");
    expect(html).toContain('data-empty="true"');
  });

  it("draws nothing to press when nothing waits anywhere", () => {
    const html = renderToStaticMarkup(
      createElement(StripBody, {
        view: {
          groups: [],
          empty: { icon: "circle-check", title: "Needs you · 0 here", text: null },
          next: null,
        },
        project: "SHOP",
      }),
    );
    expect(html).toContain("Needs you · 0 here");
    expect(html).not.toContain("<button");
  });
});

describe("pressing the strip", () => {
  it("a chip opens its target; Next asks; nothing else happens on its own", () => {
    const { onOpen, onNext } = show(VIEW);
    expect(onOpen).not.toHaveBeenCalled();
    click("allow a command");
    expect(onOpen).toHaveBeenCalledWith(ticket("SHOP-22"));
    click("your check");
    expect(onOpen).toHaveBeenLastCalledWith(ticket("SHOP-18", "files"));
    click("Next");
    expect(onNext).toHaveBeenCalledTimes(1);
    expect(ask).not.toHaveBeenCalled();
  });

  it("Resume all opens its confirmation for the strip's project", () => {
    show(VIEW);
    click("Resume all");
    expect(overlays.store.get()).toEqual([{ id: "resume_all", params: { project: "SHOP" } }]);
  });

  it("a suggestion opens the lead's card: Not now dismisses, Confirm opens the usual confirmation with its line", () => {
    ask.mockResolvedValue(null);
    show(VIEW);
    click("Close 40.6?");
    expect(document.body.textContent).toContain("Wishlist lead suggests");
    expect(document.body.textContent).toContain("PAY-123 doesn't ask for an export.");
    act(() => {
      [...document.querySelectorAll<HTMLButtonElement>(".strip-suggestion button")]
        .find((b) => b.textContent === "Confirm")
        ?.click();
    });
    expect(overlays.store.get()).toEqual([
      {
        id: "close_ticket",
        params: {
          ticket: "SHOP-40.6",
          suggestion: { id: "sg1", line: "Suggested by Wishlist lead: “PAY-123 doesn't ask for an export.”" },
        },
      },
    ]);
    expect(ask).not.toHaveBeenCalled();
    click("Close 40.6?");
    act(() => {
      [...document.querySelectorAll<HTMLButtonElement>(".strip-suggestion button")]
        .find((b) => b.textContent === "Not now")
        ?.click();
    });
    expect(ask).toHaveBeenCalledWith("strip.suggestion", { id: "sg1", confirm: false });
  });
});

describe("opening and Next", () => {
  it("a ticket opens on its tab in its project, its terminal focused on Session's sides and the Lead tab", () => {
    const r = routes("SHOP");
    openTarget(ticket("BILL-7", "agent", "BILL"), r);
    expect(r.selection.get()).toMatchObject({ project: "BILL", ticket: "BILL-7", tab: "agent" });
    expect(r.focus.get()).toEqual({ ticket: "BILL-7", seq: 1 });
    openTarget(ticket("SHOP-18", "files"), r);
    expect(r.selection.get()).toMatchObject({ project: "SHOP", ticket: "SHOP-18", tab: "files" });
    expect(r.focus.get()?.seq).toBe(1);
    openTarget(ticket("SHOP-40", "lead"), r);
    expect(r.focus.get()).toEqual({ ticket: "SHOP-40", seq: 2 });
    expect(r.last.get()).toEqual(ticket("SHOP-40", "lead"));
    // A check that needs you: Session on the check's side, focused.
    openTarget(ticket("SHOP-30", "check"), r);
    expect(r.selection.get()).toMatchObject({ ticket: "SHOP-30", tab: "check" });
    expect(r.focus.get()).toEqual({ ticket: "SHOP-30", seq: 3 });
  });

  it("⌘J takes the keyboard from the terminal that had it: a question's terminal is asked for, a review's isn't (finding 55)", () => {
    const r = routes("SHOP");
    r.selection.update((s) => ({ ...s, ticket: "SHOP-15", focusedTerminal: "s15" }));
    openTarget(ticket("SHOP-18", "files"), r);
    expect(r.blur).toHaveBeenCalledTimes(1);
    expect(r.selection.get().focusedTerminal).toBeNull();
    expect(r.focus.get()).toBeNull();
    r.selection.update((s) => ({ ...s, focusedTerminal: "s18" }));
    openTarget(ticket("SHOP-16", "agent"), r);
    expect(r.blur).toHaveBeenCalledTimes(2);
    expect(r.focus.get()).toEqual({ ticket: "SHOP-16", seq: 1 });
    // Nothing had the keyboard: nothing to take.
    openTarget(ticket("SHOP-17", "overview"), r);
    expect(r.blur).toHaveBeenCalledTimes(2);
  });

  it("a Project agent opens in the panel, focused", () => {
    const r = routes("SHOP");
    openTarget({ kind: "agent", project: "SHOP", agent: "pa3" }, r);
    expect(r.pane.shown.get()).toEqual({ SHOP: "pa3" });
    expect(r.pane.layout.get().agentOpen).toBe(true);
    expect(r.pane.focus.get()).toEqual({ seq: 1, agent: "pa3" });
    expect(openOf(r)).toEqual({ kind: "agent", agent: "pa3" });
  });

  it("what Next moves on from: the agent it opened, else the open ticket, else the open panel's conversation", () => {
    const r = routes("SHOP");
    expect(openOf(r)).toBeNull();
    r.selection.update((s) => ({ ...s, ticket: "SHOP-12" }));
    expect(openOf(r)).toEqual({ kind: "ticket", key: "SHOP-12" });
    r.selection.update((s) => ({ ...s, ticket: null }));
    r.pane.shown.set({ SHOP: "pa1" });
    r.pane.layout.set({ ...INITIAL_LAYOUT, agentOpen: true });
    expect(openOf(r)).toEqual({ kind: "agent", agent: "pa1" });
  });

  it("Next asks from what is open and opens the answer, switching project (NYS-02)", async () => {
    const r = routes("SHOP");
    r.selection.update((s) => ({ ...s, ticket: "SHOP-18" }));
    ask.mockResolvedValueOnce(ticket("BILL-7", "agent", "BILL"));
    await runNext(ask, r);
    expect(ask).toHaveBeenCalledWith("strip.next", {
      project: "SHOP",
      open: { kind: "ticket", key: "SHOP-18" },
    });
    expect(r.selection.get()).toMatchObject({ project: "BILL", ticket: "BILL-7" });
    // Nothing waits anywhere: nothing opens.
    ask.mockResolvedValueOnce(null);
    await runNext(ask, r);
    expect(r.selection.get()).toMatchObject({ project: "BILL", ticket: "BILL-7" });
    // No project yet: nothing is asked.
    ask.mockClear();
    await runNext(ask, routes(null));
    expect(ask).not.toHaveBeenCalled();
  });

  it("a suggestion's confirm and Resume all's confirm send their requests", async () => {
    ask.mockResolvedValue(null);
    await confirmSuggestion({ id: "sg1", line: "…" }, ask);
    await dismissSuggestion("sg2", ask);
    await resumeAll("SHOP", ask);
    await resumeAll(undefined, ask);
    expect(ask.mock.calls).toEqual([
      ["strip.suggestion", { id: "sg1", confirm: true }],
      ["strip.suggestion", { id: "sg2", confirm: false }],
      ["strip.resume_all", { project: "SHOP" }],
      ["notices.resume_all"],
    ]);
  });

  it("Resume all's confirmation lists what it resumes and what it leaves out", () => {
    const html = renderToStaticMarkup(
      createElement(ResumeAllBody, {
        dialog: {
          title: "Resume 3 interrupted sessions?",
          text: "Each session continues in its own worktree. This starts agents, so it uses tokens.",
          included: [{ key: "SHOP-33", title: "Shipping estimate" }],
          not_included: "Not included: SHOP-28 Cart badge count.",
          cancel: "Cancel",
          cancel_hint: "cancel",
          confirm: "Resume 3",
        },
        project: "SHOP",
        close: () => undefined,
      }),
    );
    for (const text of [
      "Resume 3 interrupted sessions?",
      "SHOP-33",
      "Shipping estimate",
      "Not included",
      "Resume 3",
    ])
      expect(html).toContain(text);
  });
});
