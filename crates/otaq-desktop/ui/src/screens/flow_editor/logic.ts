// The flow editor's moment of interaction (T-0471; FED-01, FED-03, FED-04, FED-07; d0115): where it opens, which
// step is selected, which agent the panel shows, the drag of a step, the animation's clock and the calls each click
// makes. What it says and every rule are Rust's (`flow_editor.view`, `otaq-view`'s `flow_editor.rs`); every edit of
// the user's goes to the draft through `flow_editor.edit` / `pick` / `undo` / `import` (`flow.draft_update`), and the
// Skill field's buttons through `prefill` / `write_skill` / `stop_writing` / `undo_skill` / `open_skill` (T-0472).
// Nothing here starts an agent without the user's words, a chip or an Ask (d0087); Ask the agent to write it only
// prefills the agent's box (Q441). The picks a problem needs (Q426) stay here until Apply sends them, as the preview's
// did.
import { openOverlay } from "../../app/overlays";
import { createStore, type Store } from "../../app/store";
import type { BoardColumnEditor } from "../../generated/BoardColumnEditor";
import type { FlowEditorChipAction } from "../../generated/FlowEditorChipAction";
import type { FlowEditorChoices } from "../../generated/FlowEditorChoices";
import type { FlowEditorClashPick } from "../../generated/FlowEditorClashPick";
import type { FlowEditorEdited } from "../../generated/FlowEditorEdited";
import type { FlowEditorFrom } from "../../generated/FlowEditorFrom";
import type { FlowEditorPick } from "../../generated/FlowEditorPick";
import type { FlowEditorStageEdit } from "../../generated/FlowEditorStageEdit";
import type { FlowEditorStarted } from "../../generated/FlowEditorStarted";
import type { FlowEditorView } from "../../generated/FlowEditorView";
import { request } from "../../tauri";

/** The door's request call, or a fake one in the tests. */
export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/**
 * Where the editor opens over the board (FED-09): the project, where it opened, and optionally a stage to select
 * (Edit <stage>…) or one to add a new stage after (Add a stage after <stage>, the new one selected with its details
 * open; `null`: first, after Ready for Dev). Onboarding's Flow step and the Flow setup page draw it in place instead.
 */
export interface FlowEditorOpen {
  project: string;
  from: FlowEditorFrom;
  select_stage?: string;
  add_after?: string | null;
}

/** Opens the editor over the window. */
export function openFlowEditor(open: FlowEditorOpen): void {
  openOverlay("flow_editor", open);
}

/** The project menu's Change flow…, Settings → Flow's Change flow… and a column's Change the whole flow…. */
export function changeFlowOpen(project: string): FlowEditorOpen {
  return { project, from: "project" };
}

/** How a column's ⋯ item (BRD-04) opens the editor on its project: that stage selected, a new stage after it, or the
 * whole flow. */
export function columnEditorOpen(project: string, editor: BoardColumnEditor): FlowEditorOpen {
  switch (editor.open) {
    case "stage":
      return { project, from: "project", select_stage: editor.stage };
    case "add_after":
      return { project, from: "project", add_after: editor.after };
    case "flow":
      return changeFlowOpen(project);
  }
}

/** The edit that adds the stage a column's Add a stage after <stage> asked for, on opening. */
export function addOnOpening(after: string | null): FlowEditorStageEdit {
  return after === null ? { edit: "add" } : { edit: "add", after };
}

/** The agent each project's editor panel shows, once one was asked (the page's own state, like the pane's). */
export const editorAgents: Store<Readonly<Record<string, string>>> = createStore<
  Readonly<Record<string, string>>
>({});

export function editorAgent(
  project: string,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): string | null {
  return agents.get()[project] ?? null;
}

export function showEditorAgent(
  project: string,
  agent: string,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): void {
  agents.update((all) => ({ ...all, [project]: agent }));
}

/** The service events that change what the editor shows. */
export const FLOW_EDITOR_EVENTS = [
  "flow_changed",
  "flow_setup_changed",
  "agents_changed",
  "agent_tools_changed",
  "project_agent_changed",
  "projects_changed",
  "ticket_changed",
  "card_status_changed",
  "settings_changed",
] as const;

/** A step's edit; resolves to the step to select next (a stage just added), else `null`. */
export async function editStage(
  project: string,
  edit: FlowEditorStageEdit,
  ask: Ask = request,
): Promise<string | null> {
  const edited = (await ask("flow_editor.edit", { project, edit })) as FlowEditorEdited;
  return edited.selected;
}

/** Picking a Start from entry copies it into the draft. */
export function pickEntry(project: string, pick: FlowEditorPick, ask: Ask = request): Promise<unknown> {
  return ask("flow_editor.pick", { project, pick });
}

/** Undo on a tinted step, or Undo all (`stage` null). */
export function undo(project: string, stage: string | null, ask: Ask = request): Promise<unknown> {
  return ask("flow_editor.undo", stage === null ? { project } : { project, stage });
}

/** Import a flow…'s menu choice: `file` (the open dialog, cancelling does nothing) or `project:<KEY>`. */
export async function importChoice(
  project: string,
  id: string,
  pickFile: () => Promise<string | null>,
  ask: Ask = request,
): Promise<void> {
  if (id === "file") {
    const path = await pickFile();
    if (path === null) return;
    await ask("flow_editor.import", { project, path });
    return;
  }
  if (id.startsWith("project:")) {
    await pickEntry(project, { pick: "project", project: id.slice("project:".length) }, ask);
  }
}

/** No picks yet. */
export const NO_CHOICES: FlowEditorChoices = { clashes: {}, skill_folders: {}, models: {} };

/** An import clash picked: Keep mine, Take theirs or Add as. */
export function withClash(
  choices: FlowEditorChoices,
  agent: string,
  pick: FlowEditorClashPick,
): FlowEditorChoices {
  return { ...choices, clashes: { ...choices.clashes, [agent]: pick } };
}

/** A missing skill's folder picked (Import its folder…). */
export function withFolder(choices: FlowEditorChoices, skill: string, folder: string): FlowEditorChoices {
  return { ...choices, skill_folders: { ...choices.skill_folders, [skill]: folder } };
}

/**
 * A local model picked for an agent: the default model (`default`), a tool (`claude`, its default model) or one of
 * its models (`claude:opus`).
 */
export function withModel(
  choices: FlowEditorChoices,
  agent: string,
  tool: string,
  model: string | null = null,
): FlowEditorChoices {
  const value = tool === "default" || model === null ? tool : `${tool}:${model}`;
  return { ...choices, models: { ...choices.models, [agent]: value } };
}

/**
 * Apply (Use this flow) and Cancel (Skip, use the default): both close the editor once the service answered. Apply
 * sends the picks as the view gives them back (every clash with its pick).
 */
export function applyDraft(view: FlowEditorView, ask: Ask = request): Promise<unknown> {
  return ask("flow_editor.apply", {
    project: view.project,
    from: view.from,
    proposal: view.proposal,
    choices: view.choices,
  });
}

export function cancelDraft(view: FlowEditorView, ask: Ask = request): Promise<unknown> {
  return ask("flow_editor.cancel", { project: view.project, from: view.from, proposal: view.proposal });
}

/** The words on their way, by project: a second Return while one is sending starts nothing. */
const sending = new Set<string>();

/**
 * The panel's first message (typed, or a chip's words): starts the Project agent on the `flow` skill, which the panel
 * then shows. Nothing is sent for empty words. Resolves to the agent's id, or `null` when nothing was sent.
 */
export async function sendFirst(
  project: string,
  text: string,
  ask: Ask = request,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): Promise<string | null> {
  if (text.trim() === "" || sending.has(project)) return null;
  sending.add(project);
  try {
    const started = (await ask("flow_editor.send", { project, text })) as FlowEditorStarted;
    showEditorAgent(project, started.agent, agents);
    return started.agent;
  } finally {
    sending.delete(project);
  }
}

/** Ask <Tool> (Made for, the chooser, "Ask Claude Code"); the panel shows the agent that looks. */
export async function askTool(
  project: string,
  from: FlowEditorFrom,
  tool: string,
  ask: Ask = request,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): Promise<string> {
  const started = (await ask("flow_editor.ask", { project, from, tool })) as FlowEditorStarted;
  showEditorAgent(project, started.agent, agents);
  return started.agent;
}

/** "Both, and compare": asks each tool in turn (each proposal becomes its own Made for entry). */
export async function askEach(
  project: string,
  from: FlowEditorFrom,
  tools: readonly string[],
  ask: Ask = request,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): Promise<void> {
  for (const tool of tools) await askTool(project, from, tool, ask, agents);
}

/**
 * Ask the agent to write it (and the Write the skill chip, FED-06, Q441): **nothing is sent**. With the panel's agent
 * running (`running`, its id), "Write a skill for the Plan stage: " is typed into its input line and resolves to
 * `"terminal"` (its terminal then takes the keyboard); otherwise `fillBox` puts it in the idle panel's box, and it
 * resolves to `"box"`.
 */
export async function askToWrite(
  project: string,
  stage: string,
  prefill: string,
  running: string | null,
  fillBox: (stage: string, text: string) => void,
  ask: Ask = request,
): Promise<"box" | "terminal"> {
  if (running === null) {
    fillBox(stage, prefill);
    return "box";
  }
  await ask("flow_editor.prefill", { project, stage, agent: running });
  return "terminal";
}

/**
 * The user's send from the box Ask the agent to write it prefilled: the Project agent starts with their words, which
 * the panel then shows, and only now the field says it is being written. Nothing is sent for empty words. Resolves to
 * the agent's id, or `null` when nothing was sent.
 */
export async function writeSkill(
  project: string,
  stage: string,
  text: string,
  ask: Ask = request,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): Promise<string | null> {
  if (text.trim() === "" || sending.has(project)) return null;
  sending.add(project);
  try {
    const started = (await ask("flow_editor.write_skill", { project, stage, text })) as FlowEditorStarted;
    showEditorAgent(project, started.agent, agents);
    return started.agent;
  } finally {
    sending.delete(project);
  }
}

/** The Skill field's other buttons: Stop (being written), Undo and Open (written by the agent). */
export type SkillButton = "stop_writing" | "undo_skill" | "open_skill";

/** One of them pressed. */
export function skillButton(
  project: string,
  stage: string,
  button: SkillButton,
  ask: Ask = request,
): Promise<unknown> {
  return ask(`flow_editor.${button}`, { project, stage });
}

/**
 * What a chip does: send its words, open the chooser, ask the one tool, or (Write the skill) what Ask the agent to
 * write it does, `write` (it sends nothing). `chooser` opens the tool picker.
 */
export function chipPressed(
  project: string,
  from: FlowEditorFrom,
  action: FlowEditorChipAction,
  chooser: () => void,
  write: (stage: string, prefill: string) => void,
  ask: Ask = request,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): Promise<unknown> {
  switch (action.kind) {
    case "send":
      return sendFirst(project, action.message, ask, agents);
    case "choose_tool":
      chooser();
      return Promise.resolve(null);
    case "ask":
      return askTool(project, from, action.tool, ask, agents);
    case "write_skill":
      write(action.stage, action.prefill);
      return Promise.resolve(null);
  }
}

/**
 * Where a dragged step lands (`move`'s `to`: its index among the stages once it is taken out): how many of the other
 * rows' vertical middles are above the pointer. `middles` are the stage rows' middles in order, `from` the dragged one.
 */
export function dropIndex(middles: readonly number[], y: number, from: number): number {
  return middles.filter((middle, i) => i !== from && middle < y).length;
}

/** Alt+↑ / Alt+↓ on a focused step: its new index, or `null` at the edge (or when it can't move). */
export function keyMove(index: number, count: number, key: string): number | null {
  if (key === "ArrowUp") return index > 0 ? index - 1 : null;
  if (key === "ArrowDown") return index < count - 1 ? index + 1 : null;
  return null;
}

/** How long the walking ticket stays on each tile. */
export const TILE_MS = 1600;

/** The next tile the ticket walks to, wrapping. */
export function nextTile(at: number, count: number): number {
  return count === 0 ? 0 : (at + 1) % count;
}
