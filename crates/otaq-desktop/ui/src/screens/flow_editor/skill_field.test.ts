// @vitest-environment happy-dom
// The Skill field draws the backticked parts of its texts as code: the skill nobody has in the warning, the folder
// a written skill was saved in, and Pick a skill…'s "Use `<typed text>`". Each sentence is one flowing span (T10: the
// field's flex layouts drew the parts as columns).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { FlowEditorSkillField } from "../../generated/FlowEditorSkillField";
import type { FlowEditorSkillState } from "../../generated/FlowEditorSkillState";
import type { MenuItem } from "../../generated/MenuItem";
import { Menu } from "../../components/Menu";
import { type EditorActions, SkillField } from "./FlowEditor";

const ignore = () => undefined;
const actions: EditorActions = { edit: ignore, write: ignore, skill: ignore, fix: ignore };
const FIX = { pick: "Pick one", or: ", or ", ask: "Ask the agent to write it", end: "." };
const PREFILL = "Write a skill for the Plan stage: ";

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

/** A row of Pick a skill…, as Rust sends it. */
function row(id: string, more: Partial<MenuItem> = {}): MenuItem {
  return {
    id,
    label: id,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: false,
    header: false,
    ...more,
  };
}

function show(
  state: FlowEditorSkillState,
  focus = false,
  on: EditorActions = actions,
  items: MenuItem[] = [row("validate")],
) {
  const field: FlowEditorSkillField = {
    label: "Skill",
    state,
    picker: {
      title: "Pick a skill…",
      search: { placeholder: "Search your skills…", empty: "No matching skills" },
      items,
      use_typed: "Use `{name}`",
      ask: {
        id: ":ask-the-agent",
        label: "Ask the agent to write it…",
        icon: "sparkles",
        shortcut: null,
        danger: false,
        disabled: false,
        hint: "Starts Noyan on the right and uses tokens.",
        separated: true,
        checked: false,
        header: false,
      },
      prefill: PREFILL,
    },
  };
  act(() => {
    root.render(createElement(SkillField, { field, stage: "plan", actions: on, focus }));
  });
}

const codes = () => [...box.querySelectorAll("code")].map((c) => c.textContent);

/** Opens Pick a skill… from the stage's skill and types `text` in its search. */
function search(text: string) {
  const open = box.querySelector<HTMLButtonElement>(".fe-skill-set");
  if (!open) throw new Error("no skill button");
  act(() => {
    open.click();
  });
  const field = document.querySelector<HTMLInputElement>(".menu input");
  if (!field) throw new Error("no search field");
  act(() => {
    // React reads the value through the prototype's setter, as typing sets it.
    Reflect.set(HTMLInputElement.prototype, "value", text, field);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

const rows = () => [...document.querySelectorAll<HTMLButtonElement>(".menu-item")];

/** Actions that record what Ask the agent to write it and a pick asked for. */
function recording() {
  const asked: [string, string][] = [];
  const edits: unknown[] = [];
  const on: EditorActions = {
    ...actions,
    write: (stage, prefill) => asked.push([stage, prefill]),
    edit: (edit) => edits.push(edit),
  };
  return { asked, edits, on };
}

describe("the Skill field", () => {
  it("draws the skill nobody has as code in the warning", () => {
    show({
      state: "set",
      skill: "team-validate",
      warning: "You don't have the skill `team-validate`",
      fix: null,
    });
    expect(codes()).toEqual(["team-validate"]);
    expect(box.querySelector(".fe-fix")?.textContent).toBe("You don't have the skill team-validate");
    // One span holds the sentence, so the name flows in it.
    const parts = [...(box.querySelector(".fe-fix-text")?.children ?? [])];
    expect(parts.map((p) => p.tagName)).toEqual(["SPAN"]);
    expect(parts[0]?.querySelector("code")?.textContent).toBe("team-validate");
  });

  it("offers Pick one, or Ask the agent to write it under a missing skill's warning", () => {
    const { asked, on } = recording();
    show(
      { state: "set", skill: "update-docs", warning: "You don't have the skill `update-docs`", fix: FIX },
      false,
      on,
    );
    const fix = box.querySelector(".fe-skill-fix");
    expect(fix?.textContent).toBe("Pick one, or Ask the agent to write it.");
    const [pickOne, ask] = [...(fix?.querySelectorAll("button") ?? [])];
    act(() => {
      ask?.click();
    });
    // Only the user's words box opens, prefilled: nothing is sent yet.
    expect(asked).toEqual([["plan", PREFILL]]);
    act(() => {
      pickOne?.click();
    });
    expect(document.querySelector(".menu")).not.toBeNull();
  });

  it("ends Pick a skill… with Ask the agent to write it…, searching or not", () => {
    const { asked, edits, on } = recording();
    show({ state: "set", skill: "validate", warning: null, fix: null }, false, on);
    search("");
    expect(rows().map((r) => r.querySelector(".menu-item-label")?.textContent)).toEqual([
      "validate",
      "Ask the agent to write it…",
    ]);
    search("zzz");
    expect(rows().map((r) => r.querySelector(".menu-item-label")?.textContent)).toEqual([
      "Use zzz",
      "Ask the agent to write it…",
    ]);
    act(() => {
      rows().at(-1)?.click();
    });
    expect(asked).toEqual([["plan", PREFILL]]);
    expect(edits).toEqual([]);
  });

  // Decision 108: the user's own skills first, unmarked; then the "Otaq" group, its header with Otaq's mark, each of
  // its rows (`otaq:` and `tuq:` skills) with an "Otaq" badge, searching or not.
  it("groups and badges Otaq's skills below the user's, searching or not", () => {
    const picker = [
      row("validate", { hint: "Personal · Claude Code · Checks the build." }),
      row("", { label: "Otaq", header: true, separated: true, mark: true }),
      row("otaq:validate", { badge: "Otaq", hint: "A checking agent runs the tests and lint." }),
      row("tuq:product-spec", { badge: "Otaq", hint: "An agent asks you what to build." }),
    ];
    show({ state: "set", skill: "validate", warning: null, fix: null }, false, actions, picker);
    /** The rows and headers as drawn: a header as `# label` (`#*` with the mark), a row as `label [badge]`. */
    const drawn = () =>
      [...document.querySelectorAll(".menu-header, .menu-item")].map((el) => {
        if (el.classList.contains("menu-header"))
          return `#${el.querySelector(".otaq-mark") ? "*" : ""} ${el.textContent}`;
        const badge = el.querySelector(".menu-item-badge")?.textContent;
        const label = el.querySelector(".menu-item-label")?.textContent ?? "";
        return badge === undefined ? label : `${label} [${badge}]`;
      });
    search("");
    expect(drawn()).toEqual([
      "validate",
      "#* Otaq",
      "otaq:validate [Otaq]",
      "tuq:product-spec [Otaq]",
      "Ask the agent to write it…",
    ]);
    // The user's own validate stays unmarked; the badge is only on Otaq's.
    expect(rows()[0]?.querySelector(".menu-item-badge")).toBeNull();
    search("validate");
    expect(drawn()).toEqual(["validate", "#* Otaq", "otaq:validate [Otaq]", "Ask the agent to write it…"]);
    search("product");
    expect(drawn()).toEqual([
      "#* Otaq",
      "tuq:product-spec [Otaq]",
      "Use product",
      "Ask the agent to write it…",
    ]);
    // Nothing of Otaq's matches: no header either.
    search("zzz");
    expect(drawn()).toEqual(["Use zzz", "Ask the agent to write it…"]);
  });

  // F-D-1, decision 108 refined: wide enough to read, each description cut to two lines with the whole on hover.
  it("opens Pick a skill… 360 wide, its descriptions cut to two lines with the whole on hover", () => {
    const long =
      "Address the review feedback on the Otaq ticket named by $OTAQ_TICKET after the user sent it back - get the " +
      "context and the user's comment with `otaq prime`, read the line comments, fix them and commit everything.";
    const picker = [
      row("", { label: "Otaq", header: true, mark: true }),
      row("otaq:address-review", { badge: "Otaq", hint: long }),
    ];
    show({ state: "set", skill: "validate", warning: null, fix: null }, false, actions, picker);
    search("");
    expect(document.querySelector<HTMLElement>(".menu")?.style.width).toBe("360px");
    const hint = rows()[0]?.querySelector<HTMLElement>(".menu-item-hint");
    expect(hint?.textContent).toBe(long);
    expect(hint?.hasAttribute("data-cut")).toBe(true);
    expect(hint?.style.getPropertyValue("--menu-hint-lines")).toBe("2");
    expect(hint?.title).toBe(long);
  });

  // Only Pick a skill… cuts its hints: another menu (the Approve menu, say) keeps its width and its hints whole.
  it("leaves other menus' hints whole, at their usual width", () => {
    const hint =
      "Merges the branch, then runs the checks again on the merged code before it closes the ticket.";
    act(() => {
      root.render(
        createElement(Menu, {
          label: "Approve",
          items: [row("merge", { label: "Approve and merge", hint })],
          onChoose: ignore,
          onClose: ignore,
        }),
      );
    });
    expect(document.querySelector<HTMLElement>(".menu")?.style.width).toBe("260px");
    const shown = rows()[0]?.querySelector<HTMLElement>(".menu-item-hint");
    expect(shown?.textContent).toBe(hint);
    expect(shown?.hasAttribute("data-cut")).toBe(false);
    expect(shown?.getAttribute("style")).toBeNull();
    expect(shown?.hasAttribute("title")).toBe(false);
  });

  it("draws the name typed in Pick a skill… as code in its Use row", () => {
    show({ state: "set", skill: "validate", warning: null, fix: null });
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
    const labels = [...document.querySelectorAll(".menu-item-label")];
    expect(labels.at(-2)?.textContent).toBe("Use nope");
    expect(labels.at(-2)?.querySelector("code")?.textContent).toBe("nope");
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
    show({ state: "set", skill: "open-pr", warning: null, fix: null }, true);
    expect(box.querySelector("[data-field='skill']")).not.toBeNull();
    expect(document.activeElement).toBe(box.querySelector(".fe-skill-set"));
  });

  it("isn't focused otherwise", () => {
    show({ state: "set", skill: "open-pr", warning: null, fix: null });
    expect(document.activeElement).not.toBe(box.querySelector(".fe-skill-set"));
  });
});
