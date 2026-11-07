import { beforeEach, describe, expect, it, vi } from "vitest";

import { createStore } from "../../app/store";
import {
  editStage,
  NO_CHOICES,
  pickEntry,
  skillButton,
  typedItem,
  withClash,
  withModel,
  writeSkill,
} from "./logic";

const ask = vi.fn<(method: string, params?: unknown) => Promise<unknown>>();

beforeEach(() => {
  ask.mockReset();
});

describe("the board editor's calls for its draft", () => {
  it("an edit goes to the draft and hands back the stage to select", async () => {
    ask.mockResolvedValueOnce({ selected: "new-stage" });
    const edit = { edit: "add_template", after: "dev", template: "new-stage" } as const;
    expect(await editStage("SHOP", edit, ask)).toBe("new-stage");
    expect(ask).toHaveBeenCalledWith("flow_editor.edit", { project: "SHOP", edit });
  });

  it("Start from copies the pick into the draft", async () => {
    ask.mockResolvedValue(null);
    await pickEntry("SHOP", { pick: "project", project: "BILL" }, ask);
    expect(ask).toHaveBeenCalledWith("flow_editor.pick", {
      project: "SHOP",
      pick: { pick: "project", project: "BILL" },
    });
  });

  it("the user's words for Ask the agent to write it start the agent; empty words start none", async () => {
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

  it("a problem's picks are kept until Save sends them (Q426)", () => {
    let picks = withModel(NO_CHOICES, "Rust developer", "claude", "opus");
    picks = withModel(picks, "Security reviewer", "default", "ignored");
    picks = withClash(picks, "Validator", "add_as");
    expect(picks).toEqual({
      clashes: { Validator: "add_as" },
      models: { "Rust developer": "claude:opus", "Security reviewer": "default" },
    });
    expect(NO_CHOICES).toEqual({ clashes: {}, models: {} });
  });

  it("Pick a skill… takes a name typed by hand: Use `<typed text>`, chosen as the text", () => {
    expect(typedItem("Use `{name}`", "   ")).toBeNull();
    const item = typedItem("Use `{name}`", " team-validate ");
    expect(item).toMatchObject({
      id: "team-validate",
      label: "Use `team-validate`",
      disabled: false,
      header: false,
    });
  });
});
