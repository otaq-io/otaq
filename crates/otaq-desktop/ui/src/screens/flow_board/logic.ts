// Edit flow on the board: when the board is the editor, what each click asks of Rust and the moment of interaction
// (which project is being edited, the selected column, the stack Undo walks back, the pretend ticket's step). What
// the editor says and every rule are Rust's (`flow_board.view`, `otaq-view`'s `flow_board.rs`); every edit goes to
// the project's draft (`flow_editor.edit` / `pick` / `import`, `flow_board.restore`), and nothing changes on the
// board until Save flow (`flow_editor.apply`).
import { closeOverlay } from "../../app/overlays";
import { createStore, type Store } from "../../app/store";
import type { BoardColumnEditor } from "../../generated/BoardColumnEditor";
import type { FlowBoardView } from "../../generated/FlowBoardView";
import type { FlowBoardWatchStep } from "../../generated/FlowBoardWatchStep";
import type { FlowEditorStarted } from "../../generated/FlowEditorStarted";
import type { NewToast } from "../../generated/NewToast";
import { request } from "../../tauri";
import { editorAgents, showEditorAgent } from "../flow_editor/logic";

/** The door's request call, or a fake one in the tests. */
export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/**
 * The board becomes the editor of `project`'s flow: optionally with a column selected (Edit <stage>…, a stage's
 * Skill field to focus), or the palette open after a stage (Add a stage after <stage>; `null`: right after Backlog).
 */
export interface FlowBoardOpen {
  project: string;
  select?: string;
  focusSkill?: boolean;
  addAfter?: string | null;
}

/** Which project's board is the editor now; `null` while none is. */
export const flowEditing: Store<FlowBoardOpen | null> = createStore<FlowBoardOpen | null>(null);

/** Edit flow: the board turns into the editor. */
export function openFlowBoard(open: FlowBoardOpen, store: Store<FlowBoardOpen | null> = flowEditing): void {
  store.set(open);
}

/** Where "use your own Open PR skill" goes: app Settings closes, so the board's editor shows. */
export interface StageSkillRoutes {
  closeSettings: () => void;
  open: (open: FlowBoardOpen) => void;
}

export const STAGE_SKILL_ROUTES: StageSkillRoutes = {
  closeSettings: () => {
    closeOverlay("settings");
  },
  open: (open) => {
    openFlowBoard(open);
  },
};

/**
 * "Use your own Open PR skill" (the refused Open PR toast's link): the board's editor on that stage's Skill field, or
 * on How work leaves the flow (Done) when no stage is named.
 */
export function openStageSkill(
  project: string,
  stage: string | null,
  routes: StageSkillRoutes = STAGE_SKILL_ROUTES,
): void {
  routes.closeSettings();
  routes.open(stage === null ? { project, select: "done" } : { project, select: stage, focusSkill: true });
}

/** Cancel, Save flow or Esc: the board is the board again. */
export function closeFlowBoard(store: Store<FlowBoardOpen | null> = flowEditing): void {
  store.set(null);
}

/** Whether `project`'s board is the editor now. */
export function editing(open: FlowBoardOpen | null, project: string | null): boolean {
  return open !== null && project !== null && open.project === project;
}

/** A column's ⋯ item (Edit <stage>…, Add a stage after <stage>, Change the whole flow…): the editor on it. */
export function columnOpen(project: string, editor: BoardColumnEditor): FlowBoardOpen {
  switch (editor.open) {
    case "stage":
      return { project, select: editor.stage };
    case "add_after":
      return { project, addAfter: editor.after };
    case "flow":
      return { project };
  }
}

/**
 * Where a stage dragged from `from` and dropped on the stage at `onto` goes (`move`'s `to`, its index once it is
 * taken out): onto's place, so it lands after a stage to its right and before one to its left. `null` when it
 * doesn't move.
 */
export function dropTo(from: number, onto: number): number | null {
  return from === onto ? null : onto;
}

/** Alt+← / Alt+→ on a stage's header: its new index, or `null` at the edge. */
export function keyMove(index: number, count: number, key: string): number | null {
  if (key === "ArrowLeft") return index > 0 ? index - 1 : null;
  if (key === "ArrowRight") return index < count - 1 ? index + 1 : null;
  return null;
}

/** "Step 5 of 7". */
export function stepOf(template: string, at: number, count: number): string {
  return template.replace("{n}", String(at + 1)).replace("{m}", String(count));
}

/** The pretend ticket's step, kept inside the walk when the draft changes under it. */
export function clampStep(at: number, steps: readonly FlowBoardWatchStep[]): number {
  return Math.max(0, Math.min(at, steps.length - 1));
}

/** Save flow: the draft applies (with the picks its problems got); the toast says whether it applied or waits. */
export async function saveDraft(view: FlowBoardView, ask: Ask = request): Promise<void> {
  await ask("flow_editor.apply", {
    project: view.project,
    proposal: view.proposal,
    choices: view.choices,
  });
  const toast: NewToast = {
    level: "success",
    title: view.saved.title,
    text: view.saved.text,
    link: null,
    ticket: null,
  };
  await ask("notices.toast", toast).catch(() => undefined);
}

/** Cancel and Discard: the draft goes; nothing changed. */
export function dropDraft(view: FlowBoardView, ask: Ask = request): Promise<unknown> {
  return ask("flow_editor.cancel", { project: view.project, proposal: view.proposal });
}

/** Import a flow file…: the open dialog, then its flow in the draft. Cancelling the dialog does nothing. */
export async function importFile(
  project: string,
  pickFile: () => Promise<string | null>,
  ask: Ask = request,
): Promise<boolean> {
  const path = await pickFile();
  if (path === null) return false;
  await ask("flow_editor.import", { project, path });
  return true;
}

/** Export flow file…: the save dialog offering "shop-web.flow.yaml", then the draft as it stands is written there. */
export async function exportFile(
  view: FlowBoardView,
  saveFile: (title: string, name: string) => Promise<string | null>,
  ask: Ask = request,
): Promise<boolean> {
  const path = await saveFile(view.more.export_title, view.more.export_file);
  if (path === null) return false;
  await ask("flow_board.export", { project: view.project, path });
  return true;
}

/**
 * Suggest a flow for <project>…: Noyan's agent starts on the `flow` skill and drafts one; the editor follows that
 * agent while it drafts. Resolves to the agent's id.
 */
export async function suggest(
  project: string,
  ask: Ask = request,
  agents: Store<Readonly<Record<string, string>>> = editorAgents,
): Promise<string> {
  const started = (await ask("flow_board.suggest", { project })) as FlowEditorStarted;
  showEditorAgent(project, started.agent, agents);
  return started.agent;
}

/** Ask for changes…: the user's words go back to the agent that made the suggestion. Nothing for empty words. */
export async function askForChanges(view: FlowBoardView, text: string, ask: Ask = request): Promise<boolean> {
  if (text.trim() === "") return false;
  await ask("flow_board.ask_changes", { project: view.project, proposal: view.proposal, text: text.trim() });
  return true;
}

/** Undo: the draft's document before the last edit, which the page kept. */
export function restore(project: string, yaml: string, ask: Ask = request): Promise<unknown> {
  return ask("flow_board.restore", { project, yaml });
}

/** The service events that change what the editor shows. */
export const FLOW_BOARD_EVENTS = [
  "flow_changed",
  "agents_changed",
  "agent_tools_changed",
  "project_agent_changed",
  "projects_changed",
  "settings_changed",
] as const;

/** A box on screen, as `getBoundingClientRect` gives it. */
export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Where the first-time hint hangs: at Watch a ticket's right edge (it names it, and so never covers Backlog's column),
 * but below every stage's header, so it covers no stage card. `null` before the headers are drawn.
 */
export function hintSpot(watch: Box | undefined, heads: readonly Box[]): Box | null {
  if (watch === undefined || heads.length === 0) return null;
  const bottom = Math.max(watch.top + watch.height, ...heads.map((h) => h.top + h.height));
  return { left: watch.left, top: bottom + 8, width: watch.width, height: 0 };
}
