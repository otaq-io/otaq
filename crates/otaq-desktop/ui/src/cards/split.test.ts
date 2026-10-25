// @vitest-environment happy-dom
// Split with Noyan on an epic with no children (T-0377; LEAD-13, d0091): the click sends the Project
// agent panel Rust's first message (`agent_panel.send` → `project_agent.open`) and the agent panel, shown again if hidden, shows
// the new agent. The click doesn't select the card.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, shellLayout } from "../app/layout";
import { NOTHING_SELECTED, selection } from "../app/selection";
import type { EpicCardView } from "../generated/EpicCardView";
import { request } from "../tauri";
import { agentPanelFocusRequests, shownAgents } from "../screens/agent_panel/logic";
import { EpicCard } from ".";

vi.mock("../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ask = vi.mocked(request);

/** NOTE-10 "Sharing", made by hand with no children, as `epic_card.rs` and `lead_status.rs` word it. */
const SHARING: EpicCardView = {
  key: "NOTE-10",
  pinned: false,
  external_id: null,
  priority: null,
  pill: { palette: "epic", icon: "layers", label: "Epic" },
  title: "Sharing",
  segments: [],
  progress: "0 of 0 merged",
  branch: "otaq/NOTE-10 · on main",
  children: [],
  add: "Add ticket to epic",
  actions: [],
  faded: null,
  lead: {
    epic: "NOTE-10",
    name: null,
    state: "",
    short: "",
    full: "Split with Noyan",
    tone: "none",
    icon: "bot",
    action: {
      kind: "split",
      label: "Split with Noyan",
      icon: "bot",
      split: { project: "NOTE", message: "Split NOTE-10 into tickets" },
    },
    tooltip: {
      title: "Split with Noyan",
      lines: ["Sends Noyan the first message “Split NOTE-10 into tickets”."],
    },
    opens_tab: false,
  },
  lead_asks: false,
};

let root: Root;
let box: HTMLElement;

beforeEach(() => {
  ask.mockReset();
  box = document.body.appendChild(document.createElement("div"));
  root = createRoot(box);
  selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
  shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: false });
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

describe("Split with Noyan (T-0377)", () => {
  it("sends the agent panel's first message and shows the agent panel with the new agent", async () => {
    ask.mockResolvedValue("a1");
    const onSelect = vi.fn();
    const onLeadAction = vi.fn();
    act(() => {
      root.render(createElement(EpicCard, { card: SHARING, onSelect, onLeadAction }));
    });
    const button = [...box.querySelectorAll("button")].find((b) => b.textContent === "Split with Noyan");
    if (!button) throw new Error("no Split with Noyan button");
    expect(box.querySelector(".lead-status-action")).toBe(button);
    await act(async () => {
      button.click();
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask).toHaveBeenCalledWith("agent_panel.send", {
      project: "NOTE",
      message: "Split NOTE-10 into tickets",
    });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(shownAgents.get()).toEqual({ NOTE: "a1" });
    expect(agentPanelFocusRequests.get()).toMatchObject({ agent: "a1" });
    expect(onLeadAction).not.toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("a refusal shows no new agent (the service toasts it)", async () => {
    ask.mockRejectedValue(new Error("Usage limit reached"));
    act(() => {
      root.render(createElement(EpicCard, { card: SHARING }));
    });
    await act(async () => {
      box.querySelector<HTMLButtonElement>(".lead-status-action")?.click();
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("agent_panel.send", {
      project: "NOTE",
      message: "Split NOTE-10 into tickets",
    });
    expect(shownAgents.get()).toEqual({});
  });
});
