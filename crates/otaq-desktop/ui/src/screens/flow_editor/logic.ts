// The calls Edit flow on the board makes for its draft (T-0471, T-0472; FED-04…FED-06; d0115): every edit of the
// user's goes to the draft through `flow_editor.edit` / `pick` (`flow.draft_update`), and the Skill field's buttons
// through `write_skill` / `stop_writing` / `undo_skill` / `open_skill`. Ask the agent to write it only asks for the
// user's words (Q441); nothing starts before they send. The picks a problem needs (Q426) stay on the page until Save
// sends them. What the editor says and every rule are Rust's (`flow_board.view`).
import { createStore, type Store } from "../../app/store";
import type { FlowEditorChoices } from "../../generated/FlowEditorChoices";
import type { FlowEditorClashPick } from "../../generated/FlowEditorClashPick";
import type { FlowEditorEdited } from "../../generated/FlowEditorEdited";
import type { FlowEditorPick } from "../../generated/FlowEditorPick";
import type { FlowEditorStageEdit } from "../../generated/FlowEditorStageEdit";
import type { FlowEditorStarted } from "../../generated/FlowEditorStarted";
import type { MenuItem } from "../../generated/MenuItem";
import { request } from "../../tauri";

/** The door's request call, or a fake one in the tests. */
export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/** The agent drafting or writing for each project's editor, once one was asked (the page's own state). */
export const editorAgents: Store<Readonly<Record<string, string>>> = createStore<
  Readonly<Record<string, string>>
>({});

export function showEditorAgent(
  project: string,
  agent: string,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): void {
  agents.update((all) => ({ ...all, [project]: agent }));
}

/** A stage's edit; resolves to the stage to select next (a stage just added), else `null`. */
export async function editStage(
  project: string,
  edit: FlowEditorStageEdit,
  ask: Ask = request,
): Promise<string | null> {
  const edited = (await ask("flow_editor.edit", { project, edit })) as FlowEditorEdited;
  return edited.selected;
}

/** Start from: the pick copied into the draft. */
export function pickEntry(project: string, pick: FlowEditorPick, ask: Ask = request): Promise<unknown> {
  return ask("flow_editor.pick", { project, pick });
}

/** No picks yet. */
export const NO_CHOICES: FlowEditorChoices = { clashes: {}, models: {} };

/** An import clash picked: Keep mine, Take theirs or Add as. */
export function withClash(
  choices: FlowEditorChoices,
  agent: string,
  pick: FlowEditorClashPick,
): FlowEditorChoices {
  return { ...choices, clashes: { ...choices.clashes, [agent]: pick } };
}

/**
 * Pick a skill…'s item for a name typed by hand: "Use `<typed text>`" (`template`'s `{name}` is the text), its id the
 * text. None for an empty search.
 */
export function typedItem(template: string, text: string): MenuItem | null {
  const name = text.trim();
  if (name === "") return null;
  return {
    id: name,
    label: template.replace("{name}", name),
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: true,
    checked: false,
    header: false,
  };
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

/** The words on their way, by project: a second Return while one is sending starts nothing. */
const sending = new Set<string>();

/**
 * The user's words for Ask the agent to write it: Noyan's agent starts with them, and only now the field says it is
 * being written. Nothing is sent for empty words. Resolves to the agent's id, or `null` when nothing was sent.
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
