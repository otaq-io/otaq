import { beforeEach, describe, expect, it, vi } from "vitest";

import { createStore } from "../../app/store";
import { overlays } from "../../app/overlays";
import type { FlowEditorView } from "../../generated/FlowEditorView";
import {
  addOnOpening,
  applyDraft,
  askEach,
  askToWrite,
  changeFlowOpen,
  chipPressed,
  columnEditorOpen,
  dropIndex,
  editStage,
  importChoice,
  keyMove,
  NO_CHOICES,
  nextTile,
  openFlowEditor,
  sendFirst,
  skillButton,
  withClash,
  withFolder,
  withModel,
  writeSkill,
} from "./logic";

const ask = vi.fn<(method: string, params?: unknown) => Promise<unknown>>();

beforeEach(() => {
  ask.mockReset();
});

describe("the flow editor's calls", () => {
  it("an edit goes to the draft and hands back the stage to select", async () => {
    ask.mockResolvedValueOnce({ selected: "plan" });
    expect(await editStage("SHOP", { edit: "add", after: "grill" }, ask)).toBe("plan");
    expect(ask).toHaveBeenCalledWith("flow_editor.edit", {
      project: "SHOP",
      edit: { edit: "add", after: "grill" },
    });
  });

  it("nothing starts for empty words; words start the agent the panel then shows", async () => {
    const agents = createStore<Readonly<Record<string, string>>>({});
    expect(await sendFirst("SHOP", "   ", ask, agents)).toBeNull();
    expect(ask).not.toHaveBeenCalled();
    ask.mockResolvedValueOnce({ agent: "pa-1" });
    expect(await sendFirst("SHOP", "What’s Validate?", ask, agents)).toBe("pa-1");
    expect(ask).toHaveBeenCalledWith("flow_editor.send", { project: "SHOP", text: "What’s Validate?" });
    expect(agents.get()).toEqual({ SHOP: "pa-1" });
  });

  it("a chip sends its words, opens the chooser, or asks the one tool", async () => {
    const agents = createStore<Readonly<Record<string, string>>>({});
    const chooser = vi.fn();
    await chipPressed("SHOP", "onboarding", { kind: "choose_tool" }, chooser, vi.fn(), ask, agents);
    expect(chooser).toHaveBeenCalledTimes(1);
    expect(ask).not.toHaveBeenCalled();
    ask.mockResolvedValue({ agent: "a-cc" });
    await chipPressed("SHOP", "onboarding", { kind: "ask", tool: "claude" }, chooser, vi.fn(), ask, agents);
    expect(ask).toHaveBeenCalledWith("flow_editor.ask", {
      project: "SHOP",
      from: "onboarding",
      tool: "claude",
    });
    await chipPressed(
      "SHOP",
      "project",
      { kind: "send", message: "Why is Security here?" },
      chooser,
      vi.fn(),
      ask,
      agents,
    );
    expect(ask).toHaveBeenLastCalledWith("flow_editor.send", {
      project: "SHOP",
      text: "Why is Security here?",
    });
  });

  it("Ask the agent to write it sends nothing: it fills the idle box, or types into the running agent's line", async () => {
    const fill = vi.fn();
    expect(await askToWrite("SHOP", "plan", "Write a skill for the Plan stage: ", null, fill, ask)).toBe(
      "box",
    );
    expect(fill).toHaveBeenCalledWith("plan", "Write a skill for the Plan stage: ");
    expect(ask).not.toHaveBeenCalled();
    ask.mockResolvedValue(null);
    expect(await askToWrite("SHOP", "plan", "Write a skill for the Plan stage: ", "pa-1", fill, ask)).toBe(
      "terminal",
    );
    expect(ask).toHaveBeenCalledWith("flow_editor.prefill", {
      project: "SHOP",
      stage: "plan",
      agent: "pa-1",
    });
    expect(fill).toHaveBeenCalledTimes(1);
    // The Write the skill chip does the same.
    ask.mockReset();
    const write = vi.fn();
    await chipPressed(
      "SHOP",
      "project",
      { kind: "write_skill", stage: "plan", prefill: "Write a skill for the Plan stage: " },
      vi.fn(),
      write,
      ask,
    );
    expect(write).toHaveBeenCalledWith("plan", "Write a skill for the Plan stage: ");
    expect(ask).not.toHaveBeenCalled();
  });

  it("the user's send from the prefilled box starts the agent, which the panel then shows", async () => {
    const agents = createStore<Readonly<Record<string, string>>>({});
    expect(await writeSkill("SHOP", "plan", "  ", ask, agents)).toBeNull();
    expect(ask).not.toHaveBeenCalled();
    ask.mockResolvedValue({ agent: "pa-1" });
    const text = "Write a skill for the Plan stage: list the steps before any code";
    expect(await writeSkill("SHOP", "plan", text, ask, agents)).toBe("pa-1");
    expect(ask).toHaveBeenLastCalledWith("flow_editor.write_skill", { project: "SHOP", stage: "plan", text });
    expect(agents.get()).toEqual({ SHOP: "pa-1" });
    await skillButton("SHOP", "plan", "undo_skill", ask);
    expect(ask).toHaveBeenLastCalledWith("flow_editor.undo_skill", { project: "SHOP", stage: "plan" });
  });

  it("a problem's picks are kept until Apply sends them (Q426)", async () => {
    let picks = withFolder(NO_CHOICES, "security-review", "/skills/security-review");
    picks = withModel(picks, "Rust developer", "claude", "opus");
    picks = withModel(picks, "Security reviewer", "default", "ignored");
    picks = withClash(picks, "Validator", "add_as");
    expect(picks).toEqual({
      clashes: { Validator: "add_as" },
      skill_folders: { "security-review": "/skills/security-review" },
      models: { "Rust developer": "claude:opus", "Security reviewer": "default" },
    });
    expect(NO_CHOICES).toEqual({ clashes: {}, skill_folders: {}, models: {} });
    ask.mockResolvedValue(null);
    const view = {
      project: "SHOP",
      from: "project",
      proposal: 3,
      choices: picks,
    } as unknown as FlowEditorView;
    await applyDraft(view, ask);
    expect(ask).toHaveBeenCalledWith("flow_editor.apply", {
      project: "SHOP",
      from: "project",
      proposal: 3,
      choices: picks,
    });
  });

  it("Both, and compare asks each tool", async () => {
    ask.mockResolvedValue({ agent: "x" });
    await askEach("SHOP", "onboarding", ["claude", "opencode"], ask, createStore({}));
    expect(ask.mock.calls.map((c) => (c[1] as { tool: string }).tool)).toEqual(["claude", "opencode"]);
  });

  it("Import a flow… reads a picked file, or copies another project's flow; a cancelled dialog does nothing", async () => {
    ask.mockResolvedValue(null);
    await importChoice("SHOP", "file", () => Promise.resolve(null), ask);
    expect(ask).not.toHaveBeenCalled();
    await importChoice("SHOP", "file", () => Promise.resolve("/tmp/team.yaml"), ask);
    expect(ask).toHaveBeenCalledWith("flow_editor.import", { project: "SHOP", path: "/tmp/team.yaml" });
    await importChoice("SHOP", "project:BILL", () => Promise.resolve(null), ask);
    expect(ask).toHaveBeenLastCalledWith("flow_editor.pick", {
      project: "SHOP",
      pick: { pick: "project", project: "BILL" },
    });
  });
});

describe("moving a step", () => {
  // Five stage rows, 40 px apart.
  const middles = [20, 60, 100, 140, 180];

  it("a drag lands where the pointer is among the other rows", () => {
    expect(dropIndex(middles, 10, 3)).toBe(0);
    expect(dropIndex(middles, 70, 3)).toBe(2);
    expect(dropIndex(middles, 145, 0)).toBe(3);
    expect(dropIndex(middles, 500, 0)).toBe(4);
    expect(dropIndex(middles, 120, 3)).toBe(3);
  });

  it("Alt+↑ / Alt+↓ move one place, never past the ends", () => {
    expect(keyMove(2, 5, "ArrowUp")).toBe(1);
    expect(keyMove(0, 5, "ArrowUp")).toBeNull();
    expect(keyMove(4, 5, "ArrowDown")).toBeNull();
    expect(keyMove(1, 5, "ArrowDown")).toBe(2);
    expect(keyMove(1, 5, "x")).toBeNull();
  });

  it("the walking ticket wraps", () => {
    expect(nextTile(0, 5)).toBe(1);
    expect(nextTile(4, 5)).toBe(0);
    expect(nextTile(0, 0)).toBe(0);
  });
});

describe("where the editor opens (FED-09, BRD-04)", () => {
  it("a column's ⋯: Edit <stage>… selects it; Add a stage after <stage> adds one there; Change the whole flow…", () => {
    expect(columnEditorOpen("SHOP", { open: "stage", stage: "grill" })).toEqual({
      project: "SHOP",
      from: "project",
      select_stage: "grill",
    });
    expect(columnEditorOpen("SHOP", { open: "add_after", after: "grill" })).toEqual({
      project: "SHOP",
      from: "project",
      add_after: "grill",
    });
    // Ready for Dev's: first, right after it.
    expect(columnEditorOpen("SHOP", { open: "add_after", after: null })).toEqual({
      project: "SHOP",
      from: "project",
      add_after: null,
    });
    expect(columnEditorOpen("SHOP", { open: "flow" })).toEqual(changeFlowOpen("SHOP"));
    expect(changeFlowOpen("SHOP")).toEqual({ project: "SHOP", from: "project" });
  });

  it("the stage a column asked for is added on opening, after its stage or first", () => {
    expect(addOnOpening("grill")).toEqual({ edit: "add", after: "grill" });
    expect(addOnOpening(null)).toEqual({ edit: "add" });
  });

  it("opens over the window as the flow_editor overlay", () => {
    openFlowEditor({ project: "SHOP", from: "project", select_stage: "dev" });
    expect(overlays.store.get()).toEqual([
      { id: "flow_editor", params: { project: "SHOP", from: "project", select_stage: "dev" } },
    ]);
    overlays.store.set([]);
  });
});
