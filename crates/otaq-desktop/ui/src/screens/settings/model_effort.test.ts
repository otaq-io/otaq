// The model menu's lines and picks (`model_effort_logic.ts`): pairs on top, each tool's group with `default` first,
// and what a chosen line sends back.
import { describe, expect, it } from "vitest";

import type { ModelEffortEffort } from "../../generated/ModelEffortEffort";
import type { ModelEffortMenu } from "../../generated/ModelEffortMenu";
import type { ToolAvatarView } from "../../generated/ToolAvatarView";
import { ANOTHER_ID, effortPick, modelEffortChoices, modelEffortPick } from "./model_effort_logic";

const avatar = (letters: string): ToolAvatarView => ({
  tool: letters === "CC" ? "claude_code" : "codex",
  letters,
  background: "tool-cc",
  foreground: "tool-cc-fg",
  hover: letters,
});

const MENU: ModelEffortMenu = {
  title: "Pick a model…",
  pairs_heading: "AS IN YOUR OTHER FLOWS · sets model and effort",
  pairs: [
    {
      id: "pair:claude:opus:high",
      model: "claude:opus",
      effort: "high",
      avatar: avatar("CC"),
      label: "opus · high",
      places: "Dev in shop-web and billing-api",
      selected: false,
    },
  ],
  groups: [
    {
      tool: "codex",
      heading: "CODEX",
      avatar: avatar("CX"),
      items: [
        {
          model: "codex",
          label: "default",
          detail: "your Codex account’s model",
          mono: false,
          selected: false,
        },
        { model: "codex:gpt-5.5", label: "gpt-5.5", detail: null, mono: true, selected: true },
      ],
      status: null,
    },
  ],
  missing: "Copilot and OpenCode aren’t set up",
  set_up: "Set up…",
  foot: "A model name without a version always means that tool’s newest.",
  another: null,
};

const EFFORT: ModelEffortEffort = {
  label: "Effort",
  levels: [
    { id: "", label: "Default", selected: false },
    { id: "medium", label: "Medium", selected: true },
  ],
  hint: "How hard the model thinks.",
};

describe("the model menu", () => {
  it("lists the pairs, then each tool's group with default first", () => {
    const lines = modelEffortChoices(MENU).map((c) => [c.kind, c.label, c.checked]);
    expect(lines).toEqual([
      ["heading", "AS IN YOUR OTHER FLOWS · sets model and effort", false],
      ["option", "opus · high", false],
      ["heading", "CODEX", false],
      ["option", "default", false],
      ["option", "gpt-5.5", true],
    ]);
    expect(modelEffortChoices(MENU)[1]?.detail).toBe("Dev in shop-web and billing-api");
  });

  it("a pair sets model and effort, a model sets only the model", () => {
    expect(modelEffortPick(MENU, "pair:claude:opus:high")).toEqual({
      kind: "pair",
      model: "claude:opus",
      effort: "high",
    });
    expect(modelEffortPick(MENU, "model:codex")).toEqual({ kind: "model", model: "codex" });
    expect(modelEffortPick(MENU, "h-codex")).toBeUndefined();
  });

  it("a short menu (the Noyan panel's header) shows its pairs and Another model or effort… until it is whole", () => {
    const short = { ...MENU, another: "Another model or effort…" };
    const lines = (full: boolean) =>
      modelEffortChoices(short, { full, effort: EFFORT }).map((c) => [c.kind, c.id, c.checked]);
    expect(lines(false)).toEqual([
      ["heading", "h-pairs", false],
      ["option", "pair:claude:opus:high", false],
      ["option", ANOTHER_ID, false],
    ]);
    expect(lines(true)).toEqual([
      ["heading", "h-pairs", false],
      ["option", "pair:claude:opus:high", false],
      ["heading", "h-codex", false],
      ["option", "model:codex", false],
      ["option", "model:codex:gpt-5.5", true],
      ["heading", "h-effort", false],
      ["option", "effort:", false],
      ["option", "effort:medium", true],
    ]);
    expect(effortPick("effort:medium")).toBe("medium");
    expect(effortPick("effort:")).toBeNull();
    expect(effortPick("model:codex")).toBeUndefined();
  });
});
