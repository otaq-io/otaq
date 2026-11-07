// The box for new work's moment of interaction (UX-22): what a key does, the ticket as typed (first line its title,
// the rest its description), and the three ways out: ↵ Create (`editor.save`, in Backlog with no agent), ⌘↵ Create &
// start (`editor.create_and_start`, the user's own Start) and ⇧↵ Ask Noyan to shape it (a conversation on Noyan's
// agent through `agent_panel.send`, its panel opened). What the box says is Rust's (`new_work.view`, `otaq-view`'s
// `new_work.rs`); every rule of a ticket (a title is needed, where it lands) is the editor's in Rust.
import { createStore, type Store } from "../../app/store";
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorSaved } from "../../generated/EditorSaved";
import type { EditorTarget } from "../../generated/EditorTarget";
import { request } from "../../tauri";
import {
  AGENT_PANEL_ROUTES,
  type AgentPanelRoutes,
  sendFirstMessage,
  showAgentPanel,
} from "../agent_panel/logic";
import { createAndStart } from "../editor/useEditor";

/** What the box holds: the title line and the details under it. */
export interface Typed {
  title: string;
  details: string;
}

export const EMPTY: Typed = { title: "", details: "" };

/** What a key in the box does; `null`: the field's own. */
export type BoxKey = "create" | "start" | "ask" | "new_line" | "leave";

/**
 * ↵ Create, ⌘↵ (Ctrl+↵) Create & start, ⇧↵ Ask Noyan to shape it, ⌥↵ (Alt+↵) a new line, Esc leaves the box (what
 * was typed is kept). Nothing while an input method composes.
 */
export function boxKey(e: {
  key: string;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  isComposing?: boolean;
}): BoxKey | null {
  if (e.isComposing === true) return null;
  if (e.key === "Escape") return "leave";
  if (e.key !== "Enter") return null;
  if (e.metaKey || e.ctrlKey) return "start";
  if (e.shiftKey) return "ask";
  if (e.altKey) return "new_line";
  return "create";
}

/** Text pasted or typed into the title line: its first line stays the title, the rest goes before the details. */
export function intoTitle(typed: Typed, text: string): Typed {
  const at = text.indexOf("\n");
  if (at < 0) return { ...typed, title: text };
  const rest = text.slice(at + 1);
  return {
    title: text.slice(0, at).replace(/\r$/, ""),
    details: typed.details === "" ? rest : `${rest}\n${typed.details}`,
  };
}

/** Whether anything is typed. */
export function hasWords(typed: Typed): boolean {
  return typed.title.trim() !== "" || typed.details.trim() !== "";
}

/** The ticket as typed: the title line, and the details as its description. */
export function formOf(typed: Typed): EditorForm {
  return {
    title: typed.title.trim(),
    body: typed.details.replace(/^\s*\n/, "").trimEnd(),
    priority: null,
    base_branch: null,
    choices: [],
    waits_for: [],
  };
}

/** The words Noyan gets: everything typed, the first line first. */
export function noyanText(typed: Typed): string {
  const details = typed.details.trim();
  const title = typed.title.trim();
  if (details === "") return title;
  return title === "" ? details : `${title}\n${details}`;
}

/** How a Create went: `clear` once the ticket exists, with the refusal's words when something was refused. */
export interface Outcome {
  clear: boolean;
  note: string | null;
}

/**
 * ↵ (or ⌘↵ with `start`): creates the ticket as typed in `project`. Created, the box clears; a refused start leaves the
 * ticket created in Backlog and says why; a refused create keeps what was typed and says why.
 */
export async function createTicket(
  project: string,
  typed: Typed,
  start: boolean,
  ask: typeof request = request,
): Promise<Outcome> {
  const target: EditorTarget = { mode: "create", project };
  const form = formOf(typed);
  const saved = start
    ? await createAndStart(target, form, ask)
    : ((await ask("editor.save", { target, form })) as EditorSaved);
  if (saved.outcome === "saved") return { clear: true, note: null };
  if (saved.outcome === "title_missing") return { clear: false, note: null };
  return { clear: saved.created !== null, note: saved.refusal.message };
}

/**
 * ⇧↵: the words start a conversation on Noyan's agent and its panel opens on it. Noyan writes tickets to Backlog and
 * never starts them. Resolves to whether a conversation started (the box clears then).
 */
export async function askNoyan(
  project: string,
  typed: Typed,
  ask: typeof request = request,
  routes: AgentPanelRoutes = AGENT_PANEL_ROUTES,
): Promise<boolean> {
  const id = await sendFirstMessage(project, noyanText(typed), ask, routes);
  if (id === null) return false;
  showAgentPanel(routes);
  return true;
}

/** Counts the asks to open the box and give it the keyboard (⌘N, the toolbar's New ticket, Backlog's +). */
export const openRequests: Store<number> = createStore(0);

/** Whether the box is on screen to take them: the project's board or List, with Noyan's panel closed. */
export const boxOnScreen: Store<boolean> = createStore(false);

/** Whether the board slot shows Files now: the box isn't there (in Files ⌘N is New ticket from this file). */
export const filesShown: Store<boolean> = createStore(false);

/**
 * ⌘N and the toolbar's New ticket: the box opens with the keyboard in it. Where the box isn't on screen (Noyan's panel
 * open, a full-page ticket, Files with no file picked), the full form opens instead.
 */
export function openNewWork(openForm: () => void, onScreen: Store<boolean> = boxOnScreen): void {
  if (onScreen.get()) openRequests.update((n) => n + 1);
  else openForm();
}
