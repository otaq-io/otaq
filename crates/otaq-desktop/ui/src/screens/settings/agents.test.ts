// @vitest-environment happy-dom
// Settings → Agents (T-0410; AGM-02…AGM-04): the page asks for Otaq's copy of the model lists and then reads them again
// once; ⋯ → Delete… opens the refused dialog (OK only) or the plain question; a
// row opens its agent's page, whose Role moves the role, whose Name is saved when left, and whose Effort shows only
// when the view has one; New agent's draft is saved by Add agent with what was typed; the model dropdowns pick a tool
// and a model, or None, never a heading. What this agent reads (T-0440, AGR-01) opens on the project on screen, picks
// another, and its links go back to the page, to the project's notes and to a skill's folder.
import { act, createElement, type ReactElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentDeleteDialog } from "../../generated/AgentDeleteDialog";
import type { AgentListRow } from "../../generated/AgentListRow";
import type { AgentPage } from "../../generated/AgentPage";
import type { AgentReadsView } from "../../generated/AgentReadsView";
import type { AgentsModelChoice } from "../../generated/AgentsModelChoice";
import type { AgentsSection as AgentsSectionView } from "../../generated/AgentsSection";
import type { MenuItem } from "../../generated/MenuItem";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import { request } from "../../tauri";
import { AgentsSection, optionValue } from "./Agents";
import { SettingsContext, type SettingsContextValue } from "./context";
import { dropdownChoices, pickOf } from "./ModelPicker";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  pickFolder: vi.fn(),
  pickFlowFile: vi.fn(),
  saveFlowFile: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
  windowAction: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const item = (id: string, label: string, over: Partial<MenuItem> = {}): MenuItem => ({
  id,
  label,
  icon: null,
  shortcut: null,
  danger: false,
  disabled: false,
  hint: null,
  separated: false,
  checked: false,
  header: false,
  ...over,
});

const MENU = [item("duplicate", "Duplicate"), item("delete", "Delete…", { danger: true, separated: true })];

const row = (id: string, name: string, over: Partial<AgentListRow> = {}): AgentListRow => ({
  id,
  name,
  role: null,
  came_from: "Added by you",
  model: "Claude Code · opus",
  skills: ["work-ticket", "commit"],
  used_in: "—",
  mark: null,
  mark_link: null,
  menu: MENU,
  menu_label: `More for ${name}`,
  ...over,
});

const choice = (over: Partial<AgentsModelChoice>): AgentsModelChoice => ({
  id: "x",
  kind: "option",
  tool: "claude",
  model: null,
  label: "",
  detail: null,
  mono: false,
  selected: false,
  ...over,
});

const CHOICES: AgentsModelChoice[] = [
  choice({ id: "heading:claude", kind: "heading", label: "Claude Code" }),
  choice({ id: "claude:", label: "Tool default", detail: "Otaq passes no model" }),
  choice({ id: "claude:haiku", model: "haiku", label: "Haiku · always the newest", selected: true }),
  choice({ id: "status:codex", kind: "note", tool: "codex", label: "Loading Codex's models…" }),
];

// The shapes `agents.rs` sends: the example (its view-model test).
const SECTION: AgentsSectionView = {
  title: "Agents",
  intro: "What each agent does: its instructions, its skills and its own model.",
  new_agent_label: "New agent",
  more_label: "More agent actions",
  more: [item("agents_file", "Apply the shipped agents file again")],
  default_model: {
    label: "Default model for new agents",
    description: "A new agent, or one a preset adds, starts with it.",
    dropdown: { value: "Claude Code · haiku", choices: CHOICES, read_only: false },
  },
  fallbacks: null,
  columns: ["Name", "Model", "Skills", "Used in"],
  rows: [
    row("noyan", "Noyan", {
      role: "Noyan",
      came_from: "From Otaq’s agents file",
      used_in: "every project · Noyan",
    }),
    row("validator", "Validator", { used_in: "pocket-notes · Validate" }),
    row("strong", "Strong", { came_from: "Added by you" }),
  ],
};

const REFUSED: AgentDeleteDialog = {
  kind: "refused",
  title: "Can’t delete Validator",
  text: "Validator is used in pocket-notes · Validate. Change that stage first.",
  ok_label: "OK",
};

const CONFIRM: AgentDeleteDialog = {
  kind: "confirm",
  id: "strong",
  title: "Delete Strong?",
  text: "No stage and no role uses Strong. Sessions that ran on it keep its name in their history.",
  cancel_label: "Cancel",
  delete_label: "Delete",
};

function agentPage(over: Partial<AgentPage> = {}): AgentPage {
  return {
    id: "strong",
    title: "Strong",
    role_tag: null,
    came_from: "Added by you",
    back_label: "Back to Agents",
    reads_label: "What this agent reads",
    menu_label: "More actions",
    menu: MENU,
    draft: {
      name: "Strong",
      instructions: "",
      skills: ["shipped:work-ticket"],
      tool: "claude",
      model: "opus",
      effort: null,
    },
    name: { label: "Name", value: "Strong", problem: null },
    model: {
      label: "Model",
      tool_label: "Tool",
      tool_value: "Claude Code",
      tools: [
        { id: "", label: "Default model", selected: false },
        { id: "claude", label: "Claude Code", selected: true },
      ],
      default_now: null,
      picker: {
        value: { label: "opus", detail: null, mono: true, no_longer_listed: null },
        groups: [
          {
            heading: null,
            items: [{ value: null, label: "Tool default", detail: null, mono: false, selected: false }],
            empty: null,
          },
        ],
        status: null,
        hint: "From Claude Code's own list.",
      },
      effort: {
        label: "Effort",
        value: "Tool default",
        options: [
          { id: "", label: "Tool default", selected: true },
          { id: "high", label: "high", selected: false },
        ],
      },
      problem: null,
    },
    role: {
      label: "Role",
      options: [
        { id: "", label: "None", selected: true },
        { id: "noyan", label: "Noyan", selected: false },
        { id: "epic-lead", label: "Epic lead", selected: false },
      ],
      hint: "Each role is held by exactly one agent. Giving a role here takes it from the agent that has it.",
      moved: null,
      problem: null,
    },
    instructions: {
      label: "Instructions",
      value: "",
      placeholder: "What this agent does and how, in Markdown",
      markdown_label: "Markdown",
    },
    skills: {
      label: "Skills",
      rows: [
        {
          key: "shipped:work-ticket",
          name: "work-ticket",
          source: null,
          warnings: [],
          reimport_id: null,
          reimport_label: "Re-import",
          reimport: null,
          remove_label: "Remove",
        },
      ],
      add_label: "Add skill",
      add_options: ["commit"],
      import_label: "Import skill folder…",
      picker_title: "Import skill folder",
      problem: null,
    },
    used_in: { label: "Used in", items: [], empty: "—" },
    mark: null,
    note: "Edits apply to the next session that starts.",
    add: null,
    problem: null,
    ...over,
  };
}

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
  document.body.innerHTML = "";
  vi.mocked(request).mockReset();
});

function show(element: ReactElement) {
  act(() => {
    root.render(element);
  });
}

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

function click(target: Element | null | undefined) {
  if (!target) throw new Error("nothing to click");
  act(() => {
    (target as HTMLElement).click();
  });
}

const byText = (selector: string, text: string) =>
  [...document.querySelectorAll(selector)].find((el) => el.textContent.trim() === text);

/** Answers the page's calls as the desktop would, recording them. */
function answer(replies: Record<string, (params: unknown) => unknown>) {
  const calls: [string, unknown][] = [];
  vi.mocked(request).mockImplementation((method: string, params: unknown = null) => {
    calls.push([method, params]);
    const reply = replies[method];
    return reply ? Promise.resolve(reply(params)) : Promise.reject(new Error(`unexpected ${method}`));
  });
  return calls;
}

async function openSection(replies: Record<string, (params: unknown) => unknown> = {}) {
  const calls = answer({ "settings.agents": () => SECTION, ...replies });
  show(createElement(AgentsSection));
  await settle();
  return calls;
}

async function openMenu(name: string, entry: string) {
  click(document.querySelector(`[aria-label="More for ${name}"]`));
  click(byText(".menu-item", entry));
  await settle();
}

describe("the model dropdowns", () => {
  it("list each tool under its name, its models and its note, and pick only a model or None", () => {
    expect(dropdownChoices(CHOICES).map((c) => [c.kind, c.label, c.checked])).toEqual([
      ["heading", "Claude Code", false],
      ["option", "Tool default", false],
      ["option", "Haiku · always the newest", true],
      ["note", "Loading Codex's models…", false],
    ]);
    expect(pickOf(CHOICES, "claude:haiku")).toEqual({ tool: "claude", model: "haiku" });
    expect(pickOf(CHOICES, "claude:")).toEqual({ tool: "claude", model: null });
    expect(pickOf(CHOICES, "heading:claude")).toBeUndefined();
    expect(pickOf([choice({ id: "none", tool: null, label: "None" })], "none")).toBeNull();
    expect(optionValue("")).toBeNull();
    expect(optionValue("high")).toBe("high");
  });
});

describe("the Agents page", () => {
  it("shows Otaq's copy of the model lists, then reads them again once", async () => {
    const calls = await openSection();
    expect(calls).toHaveLength(2);
    expect(calls).toContainEqual(["settings.agents", { refresh: false }]);
    expect(calls).toContainEqual(["settings.agents", { refresh: true }]);
    expect([...document.querySelectorAll('[role="columnheader"]')].map((c) => c.textContent)).toEqual([
      "Name",
      "Model",
      "Skills",
      "Used in",
    ]);
    expect(document.querySelectorAll(".settings-agents-row:not(.settings-agents-head)")).toHaveLength(3);
    expect(document.querySelector(".settings-agents-role")?.textContent).toBe("Noyan");
    expect(document.body.textContent).toContain("From Otaq’s agents file");
  });

  it("⋯ → Apply the shipped agents file again proposes it for the project on screen (FLW-10)", async () => {
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    const calls = await openSection({ "settings.flow_propose": () => null });
    click(document.querySelector('[aria-label="More agent actions"]'));
    click(byText(".menu-item", "Apply the shipped agents file again"));
    await settle();
    expect(calls).toContainEqual([
      "settings.flow_propose",
      { project: "NOTE", source: { from: "agents_file" } },
    ]);
    selection.set(NOTHING_SELECTED);
  });

  it("⋯ → Delete… of an agent in use is refused with OK only", async () => {
    const calls = await openSection({ "settings.agent_delete_dialog": () => REFUSED });
    await openMenu("Validator", "Delete…");
    expect(calls).toContainEqual(["settings.agent_delete_dialog", { id: "validator" }]);
    expect(document.querySelector(".dialog-title")?.textContent).toBe("Can’t delete Validator");
    expect(document.body.textContent).toContain("Change that stage first.");
    expect(byText("button", "Delete")).toBeUndefined();
    click(byText("button", "OK"));
    expect(document.querySelector(".dialog")).toBeNull();
  });

  it("⋯ → Delete… of an unused agent asks once, and Delete deletes it", async () => {
    const calls = await openSection({
      "settings.agent_delete_dialog": () => CONFIRM,
      "settings.agent_delete": () => ({
        outcome: "deleted",
        section: { ...SECTION, rows: SECTION.rows.slice(0, 2) },
      }),
    });
    await openMenu("Strong", "Delete…");
    click(byText("button", "Delete"));
    await settle();
    expect(calls).toContainEqual(["settings.agent_delete", { id: "strong" }]);
    expect(document.querySelectorAll(".settings-agents-row:not(.settings-agents-head)")).toHaveLength(2);
  });

  it("⋯ → Duplicate opens a draft from the agent", async () => {
    const calls = await openSection({
      "settings.agent": () =>
        agentPage({ id: null, title: "New agent", role: null, used_in: null, note: null }),
    });
    await openMenu("Strong", "Duplicate");
    expect(calls).toContainEqual([
      "settings.agent",
      { id: null, duplicate: "strong", draft: null, change: null },
    ]);
    expect(document.querySelector(".settings-agent-page")).not.toBeNull();
  });
});

describe("one agent's page", () => {
  async function openAgent(page: AgentPage, replies: Record<string, (params: unknown) => unknown> = {}) {
    const calls = await openSection({ "settings.agent": () => page, ...replies });
    click(byText(".settings-agents-open span", "Strong")?.closest("button"));
    await settle();
    return calls;
  }

  it("opens on the agent Settings was opened on (Open agent…, T-0411)", async () => {
    const calls = answer({ "settings.agents": () => SECTION, "settings.agent": () => agentPage() });
    const context = { agent: "strong" } as unknown as SettingsContextValue;
    show(createElement(SettingsContext, { value: context }, createElement(AgentsSection)));
    await settle();
    expect(calls).toContainEqual([
      "settings.agent",
      { id: "strong", duplicate: null, draft: null, change: null },
    ]);
  });

  it("opens from its row and moves a role with one request, saying whose it was", async () => {
    const role = agentPage().role;
    if (!role) throw new Error("a saved agent has a role field");
    const moved = agentPage({
      role_tag: "Noyan",
      role: {
        ...role,
        options: [
          { id: "", label: "None", selected: false },
          { id: "noyan", label: "Noyan", selected: true },
          { id: "epic-lead", label: "Epic lead", selected: false },
        ],
        moved: "Strong is now Noyan · the agent Noyan has no role",
      },
    });
    const calls = await openAgent(agentPage(), { "settings.agent_role": () => moved });
    expect(calls).toContainEqual([
      "settings.agent",
      { id: "strong", duplicate: null, draft: null, change: null },
    ]);
    click(byText('[role="radio"]', "Noyan"));
    await settle();
    expect(calls).toContainEqual(["settings.agent_role", { id: "strong", role: "noyan" }]);
    expect(document.querySelector(".settings-agent-moved")?.textContent).toBe(
      "Strong is now Noyan · the agent Noyan has no role",
    );
  });

  it("saves the name when its field is left, and shows Effort only when the view has it", async () => {
    const calls = await openAgent(agentPage(), { "settings.agent": () => agentPage() });
    expect(byText(".settings-agent-effort span", "Effort")).not.toBeUndefined();
    const name = document.querySelector<HTMLInputElement>("#agent-name");
    if (!name) throw new Error("no name field");
    act(() => {
      // React reads the value through the prototype's setter, as typing sets it.
      Reflect.set(HTMLInputElement.prototype, "value", "Stronger", name);
      name.dispatchEvent(new Event("input", { bubbles: true }));
    });
    act(() => {
      name.focus();
      name.blur();
    });
    await settle();
    expect(calls).toContainEqual([
      "settings.agent",
      { id: "strong", duplicate: null, draft: null, change: { field: "name", value: "Stronger" } },
    ]);
    // OpenCode has no effort level: the view sends none, and the page draws none.
    show(createElement("div"));
    await openAgent(agentPage({ model: { ...agentPage().model, effort: null } }));
    expect(document.querySelector(".settings-agent-effort")).toBeNull();
  });

  it("New agent's draft keeps its changes in the draft and Add agent sends it", async () => {
    const draft = agentPage({
      id: null,
      title: "New agent",
      menu: [],
      role: null,
      used_in: null,
      note: null,
      add: { add_label: "Add agent", cancel_label: "Cancel" },
    });
    const calls = await openSection({
      "settings.agent": () => draft,
      "settings.agent_add": () => ({ outcome: "added", page: agentPage() }),
    });
    click(byText("button", "New agent"));
    await settle();
    expect(calls).toContainEqual([
      "settings.agent",
      { id: null, duplicate: null, draft: null, change: null },
    ]);
    click(byText("button", "Add agent"));
    await settle();
    expect(calls).toContainEqual(["settings.agent_add", { draft: draft.draft }]);
    expect(byText("button", "Add agent")).toBeUndefined();
  });
});

describe("what this agent reads", () => {
  const reads = (project: string, name: string): AgentReadsView => ({
    agent_id: "strong",
    title: "What this agent reads",
    subtitle: "Strong · read-only",
    back_label: "Back to Strong",
    project_label: "Project",
    projects: [
      item("NOTE", "pocket-notes", { checked: project === "NOTE" }),
      item("API", "pocket-api", { checked: project === "API" }),
    ],
    project,
    project_name: name,
    project_hint: "Notes differ per project; the agent is shared.",
    parts: [
      {
        heading: "Its instructions",
        link: { label: "Edit in Agents →", target: { kind: "agent" } },
        text: "Work one ticket at a time.",
        empty: null,
        mono: false,
      },
      {
        heading: `Notes for ${name}`,
        link: { label: `Edit in ${name}’s settings →`, target: { kind: "project_notes", project } },
        text: null,
        empty: `No notes for ${name}`,
        mono: false,
      },
      {
        heading: "Skill: work-ticket · Dev",
        link: { label: "Open skill folder →", target: { kind: "folder", skill: "work-ticket" } },
        text: "# work-ticket",
        empty: null,
        mono: true,
      },
    ],
    no_stage: null,
    also: {
      label: "Also has:",
      skills: [{ name: "commit", note: "(commits with it)", link: { kind: "folder", skill: "commit" } }],
    },
    empty: null,
    not_shown: "Not shown: the ticket and what earlier stages wrote. They change per ticket.",
  });

  async function openReads(context: Partial<SettingsContextValue> = {}) {
    selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
    const calls = answer({
      "settings.agents": () => SECTION,
      "settings.agent": () => agentPage(),
      "settings.agent_reads": (p) =>
        (p as { project: string }).project === "API"
          ? reads("API", "pocket-api")
          : reads("NOTE", "pocket-notes"),
      "settings.agent_reads_reveal": () => null,
    });
    show(
      createElement(
        SettingsContext,
        { value: { agent: "strong", ...context } as unknown as SettingsContextValue },
        createElement(AgentsSection),
      ),
    );
    await settle();
    click(byText("button", "What this agent reads"));
    await settle();
    selection.set(NOTHING_SELECTED);
    return calls;
  }

  it("opens on the project on screen, picks another, and reveals a skill's folder", async () => {
    const calls = await openReads();
    expect(calls).toContainEqual(["settings.agent_reads", { id: "strong", project: "NOTE" }]);
    expect([...document.querySelectorAll(".settings-reads-heading")].map((h) => h.textContent)).toEqual([
      "Its instructions",
      "Notes for pocket-notes",
      "Skill: work-ticket · Dev",
    ]);
    expect(byText(".settings-reads-text", "No notes for pocket-notes")?.getAttribute("data-empty")).toBe(
      "true",
    );
    expect(document.querySelector(".settings-reads-also")?.textContent).toBe(
      "Also has:commit(commits with it)",
    );
    click(document.querySelector('[aria-label="Project"]'));
    click(byText(".menu-item", "pocket-api"));
    await settle();
    expect(calls).toContainEqual(["settings.agent_reads", { id: "strong", project: "API" }]);
    expect(byText(".settings-reads-heading", "Notes for pocket-api")).not.toBeUndefined();
    click(byText("button", "Open skill folder →"));
    click(byText("button", "commit"));
    await settle();
    expect(calls).toContainEqual([
      "settings.agent_reads_reveal",
      { id: "strong", project: "API", skill: "work-ticket" },
    ]);
    expect(calls).toContainEqual([
      "settings.agent_reads_reveal",
      { id: "strong", project: "API", skill: "commit" },
    ]);
  });

  it("goes to the project's notes, and back to the agent's page", async () => {
    const openProject = vi.fn();
    await openReads({ openProject });
    click(byText("button", "Edit in pocket-notes’s settings →"));
    expect(openProject).toHaveBeenCalledWith("NOTE", "project_notes");
    click(byText("button", "Edit in Agents →"));
    expect(document.querySelector(".settings-reads")).toBeNull();
    expect(document.querySelector(".settings-agent-page")).not.toBeNull();
  });
});
