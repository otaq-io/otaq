// The Flow step and the Flow setup page (ASK-04, FED-09, d0115): both are the flow editor from Otaq default (`FlowEditorPage`,
// `flow_editor.*` with `from: "onboarding"`, whose calls `flow_editor/flow_editor.test.ts` covers). Use this flow or
// Skip, use the default ends the step on the project's board, with the agent the editor's panel showed open; a
// project added later is selected, so its Flow setup page shows; leaving to another project and coming back finds the
// page again.
import { describe, expect, it } from "vitest";

import { INITIAL_LAYOUT, type ShellLayout } from "../../app/layout";
import { createSelection, selectProject } from "../../app/selection";
import { createStore } from "../../app/store";
import type { FlowSetupPageView } from "../../generated/FlowSetupPageView";
import type { PaneFocusRequest, PaneRoutes, ShownAgents } from "../pane/logic";
import { mainAreaShows } from "./flow_setup_page";
import { addedLater, afterFlowStep, STEP_ORDER } from "./logic";

function routes(): PaneRoutes {
  return {
    selection: createSelection(),
    layout: createStore<ShellLayout>({ ...INITIAL_LAYOUT, agentOpen: false }),
    shown: createStore<ShownAgents>({}),
    focus: createStore<PaneFocusRequest | null>(null),
  };
}

/** `onboarding.flow_setup_page` for `project`. */
const page = (project: string, shows: boolean): FlowSetupPageView => ({ project, shows });

describe("the Flow step", () => {
  it("is the fifth and last step; the board follows it", () => {
    expect(STEP_ORDER).toEqual(["welcome", "theme", "tools", "project", "flow"]);
  });

  it("Skip, use the default with nothing asked: the board shows with no agent", () => {
    const r = routes();
    afterFlowStep("SHOP", null, r);
    expect(r.selection.get().project).toBe("SHOP");
    expect(r.shown.get()).toEqual({});
    expect(r.layout.get().agentOpen).toBe(false);
  });

  it("Use this flow after a tool was asked: the agent the editor showed stays open in the panel", () => {
    const r = routes();
    afterFlowStep("SHOP", "a-cc", r);
    expect(r.selection.get().project).toBe("SHOP");
    expect(r.shown.get()).toEqual({ SHOP: "a-cc" });
    expect(r.layout.get().agentOpen).toBe(true);
  });
});

describe("the Flow setup page", () => {
  it("a project added later is selected, so its page (the editor) shows instead of the board", () => {
    const r = routes();
    selectProject("SHOP", r.selection);
    addedLater("BILL", r);
    expect(r.selection.get().project).toBe("BILL");
    expect(mainAreaShows(page("BILL", true), "BILL", false)).toBe("page");
    // Use this flow or Skip: `projects_changed` asks again and the project is settled.
    expect(mainAreaShows(page("BILL", false), "BILL", false)).toBe("board");
  });

  it("leaving to another project and coming back → the page", () => {
    const selection = createSelection();
    selectProject("BILL", selection);
    expect(mainAreaShows(page("BILL", true), selection.get().project, false)).toBe("page");
    selectProject("SHOP", selection);
    // The answer for billing-api isn't shop-web's: nothing until shop-web's comes, then its board.
    expect(mainAreaShows(page("BILL", true), selection.get().project, false)).toBe("nothing");
    expect(mainAreaShows(page("SHOP", false), selection.get().project, false)).toBe("board");
    selectProject("BILL", selection);
    expect(mainAreaShows(page("SHOP", false), selection.get().project, false)).toBe("nothing");
    expect(mainAreaShows(page("BILL", true), selection.get().project, false)).toBe("page");
  });

  it("draws nothing behind onboarding's Flow step, and the board with no project", () => {
    expect(mainAreaShows(page("BILL", true), "BILL", true)).toBe("nothing");
    expect(mainAreaShows(page("SHOP", false), "SHOP", true)).toBe("board");
    expect(mainAreaShows(undefined, null, false)).toBe("board");
    expect(mainAreaShows(undefined, "BILL", false)).toBe("nothing");
  });
});
