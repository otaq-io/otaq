// @vitest-environment happy-dom
// The Skill field draws the backticked parts of its texts as code: the skill nobody has in the warning, the folder
// a written skill was saved in, and Pick a skill…'s "Use `<typed text>`". Each sentence is one flowing span (T10: the
// field's flex layouts drew the parts as columns).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { FlowEditorSkillField } from "../../generated/FlowEditorSkillField";
import type { FlowEditorSkillState } from "../../generated/FlowEditorSkillState";
import { type EditorActions, SkillField } from "./FlowEditor";

const ignore = () => undefined;
const actions: EditorActions = { edit: ignore, write: ignore, skill: ignore, fix: ignore };

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let box: HTMLElement;

beforeEach(() => {
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

function show(state: FlowEditorSkillState, focus = false) {
  const field: FlowEditorSkillField = {
    label: "Skill",
    state,
    picker: {
      title: "Pick a skill…",
      search: { placeholder: "Search your skills…", empty: "No matching skills" },
      items: [],
      use_typed: "Use `{name}`",
    },
  };
  act(() => {
    root.render(createElement(SkillField, { field, stage: "plan", actions, focus }));
  });
}

const codes = () => [...box.querySelectorAll("code")].map((c) => c.textContent);

describe("the Skill field", () => {
  it("draws the skill nobody has as code in the warning", () => {
    show({ state: "set", skill: "team-validate", warning: "You don't have the skill `team-validate`" });
    expect(codes()).toEqual(["team-validate"]);
    expect(box.querySelector(".fe-fix")?.textContent).toBe("You don't have the skill team-validate");
    // One span holds the sentence, so the name flows in it.
    const parts = [...(box.querySelector(".fe-fix-text")?.children ?? [])];
    expect(parts.map((p) => p.tagName)).toEqual(["SPAN"]);
    expect(parts[0]?.querySelector("code")?.textContent).toBe("team-validate");
  });

  it("draws the name typed in Pick a skill… as code in its Use row", () => {
    show({ state: "set", skill: "validate", warning: null });
    const open = box.querySelector<HTMLButtonElement>(".fe-skill-set");
    if (!open) throw new Error("no skill button");
    act(() => {
      open.click();
    });
    const search = document.querySelector<HTMLInputElement>(".menu input");
    if (!search) throw new Error("no search field");
    act(() => {
      // React reads the value through the prototype's setter, as typing sets it.
      Reflect.set(HTMLInputElement.prototype, "value", "nope", search);
      search.dispatchEvent(new Event("input", { bubbles: true }));
    });
    const rows = [...document.querySelectorAll(".menu-item-label")];
    expect(rows.at(-1)?.textContent).toBe("Use nope");
    expect(rows.at(-1)?.querySelector("code")?.textContent).toBe("nope");
  });

  it("draws the folder a written skill was saved in as code, with Open", () => {
    show({
      state: "written",
      skill: "plan-ticket",
      note: "Written by the agent",
      saved:
        "Saved in `.claude/skills/plan-ticket` in this project. Commit it so agents get it in their worktrees.",
      why: "Written by Checkout slowness",
      open: "Open",
      undo: "Undo",
    });
    expect(codes()).toEqual([".claude/skills/plan-ticket"]);
    expect(box.textContent).toContain("Saved in .claude/skills/plan-ticket in this project.");
    // The hint holds its icon and one span with the whole sentence, so it flows with the folder inline.
    const hint = [...(box.querySelector(".fe-hint")?.childNodes ?? [])].filter((c) => c.textContent !== "");
    expect(hint.map((c) => [c.nodeName, c.textContent])).toEqual([
      [
        "SPAN",
        "Saved in .claude/skills/plan-ticket in this project. Commit it so agents get it in their worktrees.",
      ],
    ]);
    expect([...box.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Open", "Undo"]);
  });

  it("has no saved line and no Open for a skill an older Otaq wrote", () => {
    show({
      state: "written",
      skill: "plan-ticket",
      note: "Written by the agent",
      saved: null,
      why: "Written by Checkout slowness",
      open: null,
      undo: "Undo",
    });
    expect(codes()).toEqual([]);
    expect([...box.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Undo"]);
  });

  it("opened on it (use your own Open PR skill), its first button gets the keyboard", () => {
    show({ state: "set", skill: "open-pr", warning: null }, true);
    expect(box.querySelector("[data-field='skill']")).not.toBeNull();
    expect(document.activeElement).toBe(box.querySelector(".fe-skill-set"));
  });

  it("isn't focused otherwise", () => {
    show({ state: "set", skill: "open-pr", warning: null });
    expect(document.activeElement).not.toBe(box.querySelector(".fe-skill-set"));
  });
});
