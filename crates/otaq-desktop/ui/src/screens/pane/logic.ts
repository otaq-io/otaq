// The Project agent panel's moment of interaction (T-0330, T-0458; SHL-04, SHB-03, SHB-04, d0087, d0113): which
// conversation each project shows, opening and closing the panel, the requests to give it the keyboard (⌘L, ⇧⌘L, an
// strip item, the Ask bar), and Send's guard against a second start. Everything the panel says, and where a project
// opens the first time (`opens_on`), is Rust's (`pane.view`, `otaq-view`'s `agent_pane.rs`). Nothing here starts an
// agent except Send with words, the user's go-ahead (d0087, d0018). The code calls the panel "the pane", as before.
import { setAgentExpanded, setAgentOpen, type ShellLayout, shellLayout } from "../../app/layout";
import { type Selection, selectProject, selection } from "../../app/selection";
import { createStore, type Store } from "../../app/store";
import type { LeadSplit } from "../../generated/LeadSplit";
import type { PaneSendParams } from "../../generated/PaneSendParams";
import type { request } from "../../tauri";

/**
 * What each project shows, by project key: an agent's id, or `null` for the empty pane ("+", ⇧⌘L). A project not in
 * the map has no choice yet and opens on the view's `opens_on`.
 */
export type ShownAgents = Readonly<Record<string, string | null>>;

export const shownAgents: Store<ShownAgents> = createStore<ShownAgents>({});

/** The agent `pane.view` is asked for: the choice for `project`, else none (the view then says where it opens). */
export function chosenAgent(shown: ShownAgents, project: string): string | null {
  return shown[project] ?? null;
}

/** Whether the page chose for `project` yet (the empty pane is a choice too). */
export function hasChoice(shown: ShownAgents, project: string): boolean {
  return project in shown;
}

/** Shows `agent` in `project`'s pane, or the empty pane for `null`. */
export function showAgent(
  project: string,
  agent: string | null,
  store: Store<ShownAgents> = shownAgents,
): void {
  store.update((s) => (hasChoice(s, project) && s[project] === agent ? s : { ...s, [project]: agent }));
}

/**
 * The named agent the empty pane starts on after "New agent on …" (M23, SHL-04), by project key: `pane.view`'s
 * `start_on` and `pane.send`'s `agent`. A project not in the map starts on its Project agent default.
 */
export type StartOn = Readonly<Record<string, string>>;

export const startOn: Store<StartOn> = createStore<StartOn>({});

/** Drops `project`'s "New agent on …" choice: the empty pane starts on the Project agent default again. */
export function clearStartOn(project: string, store: Store<StartOn> = startOn): void {
  store.update((s) => {
    if (!(project in s)) return s;
    return Object.fromEntries(Object.entries(s).filter(([key]) => key !== project));
  });
}

/** A request to give the pane the keyboard: the shown agent's terminal, or the empty pane's box. */
export interface PaneFocusRequest {
  /** Counts up with each request, so asking twice is two requests. */
  seq: number;
  /** Wait until the pane shows this agent (`null`: the empty pane) before focusing; absent: whatever it shows. */
  agent?: string | null;
}

export const paneFocusRequests: Store<PaneFocusRequest | null> = createStore<PaneFocusRequest | null>(null);

export function requestPaneFocus(
  agent?: string | null,
  store: Store<PaneFocusRequest | null> = paneFocusRequests,
): void {
  store.update((last) => ({ seq: (last?.seq ?? 0) + 1, ...(agent !== undefined && { agent }) }));
}

/** Whether a focus request is for what the pane shows now. */
export function focusFits(asked: PaneFocusRequest, shown: string | null): boolean {
  return asked.agent === undefined || asked.agent === shown;
}

/** What the pane gives the keyboard to: the shown agent's terminal while it has a session, else the box (if any). */
export function focusTarget(view: {
  running: { session: string | null } | null;
}): "terminal" | "box" | "none" {
  if (view.running === null) return "box";
  return view.running.session !== null ? "terminal" : "none";
}

/** Where the pane's keys and routes land; the tests pass their own stores. */
export interface PaneRoutes {
  selection: Store<Selection>;
  layout: Store<ShellLayout>;
  shown: Store<ShownAgents>;
  focus: Store<PaneFocusRequest | null>;
}

export const PANE_ROUTES: PaneRoutes = {
  selection,
  layout: shellLayout,
  shown: shownAgents,
  focus: paneFocusRequests,
};

/** Opens the panel (d0113); the rest of the layout stays. */
export function showPane(routes: PaneRoutes = PANE_ROUTES): void {
  setAgentOpen(true, routes.layout);
}

/** Folds the panel back into the Ask bar: its close icon, ⌘L from inside it, Esc (d0113). The conversation is kept. */
export function closePane(routes: PaneRoutes = PANE_ROUTES): void {
  setAgentOpen(false, routes.layout);
}

/** The header's Expand and its way back (T-0502): the open panel over the board at full width, or beside it again. */
export function toggleExpandPane(routes: PaneRoutes = PANE_ROUTES): void {
  setAgentExpanded(!routes.layout.get().agentExpanded, routes.layout);
}

/**
 * Esc while the panel is expanded (T-0502), wherever the keyboard is outside a terminal (in one, Esc is the agent's):
 * back beside the board, still open. Whether it took the key.
 */
export function leaveExpand(routes: PaneRoutes = PANE_ROUTES): boolean {
  if (!routes.layout.get().agentExpanded) return false;
  setAgentExpanded(false, routes.layout);
  return true;
}

/**
 * ⌘L (SHB-04): opens the panel and gives the keyboard to the shown conversation's terminal or the empty panel's box;
 * pressed while the panel has the keyboard (`inPanel`), it closes it.
 */
export function focusPane(routes: PaneRoutes = PANE_ROUTES, inPanel = false): boolean {
  if (routes.selection.get().project === null) return false;
  if (routes.layout.get().agentOpen && inPanel) {
    closePane(routes);
    return true;
  }
  showPane(routes);
  requestPaneFocus(undefined, routes.focus);
  return true;
}

/** Whether `element` is in the Project agent panel (the frame's `pane` slot). */
export function inPanel(element: Element | null): boolean {
  return element?.closest('[data-slot="agent_panel"]') != null;
}

/** The rail's Project agent button: opens the panel with the keyboard in it, or closes an open one. */
export function toggleAgentPanel(routes: PaneRoutes = PANE_ROUTES): boolean {
  if (routes.layout.get().agentOpen) {
    closePane(routes);
    return true;
  }
  return focusPane(routes);
}

/** ⇧⌘L and +: the empty pane, its box focused. Nothing is asked of the service, so nothing starts (d0087). */
export function newProjectAgent(routes: PaneRoutes = PANE_ROUTES, starts: Store<StartOn> = startOn): boolean {
  const { project } = routes.selection.get();
  if (project === null) return false;
  clearStartOn(project, starts);
  showAgent(project, null, routes.shown);
  showPane(routes);
  requestPaneFocus(null, routes.focus);
  return true;
}

/**
 * A strip item, an OS notification of a Project agent, or the Ask bar's "New reply": its project on screen, the agent
 * in the panel (opened) and its terminal focused (PAG-07, ATT-03: the focus clears its wait).
 */
export function showAgentInPane(project: string, agent: string, routes: PaneRoutes = PANE_ROUTES): void {
  selectProject(project, routes.selection);
  showAgent(project, agent, routes.shown);
  showPane(routes);
  requestPaneFocus(agent, routes.focus);
}

/**
 * "New agent on Sonnet" in a notice (SHL-04, M23): the empty pane on that named agent, its box focused. Nothing
 * starts until the user sends (d0087); the first message then starts it on `agent`.
 */
export function startNewAgentOn(
  project: string,
  agent: string,
  routes: PaneRoutes = PANE_ROUTES,
  starts: Store<StartOn> = startOn,
): void {
  starts.update((s) => (s[project] === agent ? s : { ...s, [project]: agent }));
  showAgent(project, null, routes.shown);
  showPane(routes);
  requestPaneFocus(null, routes.focus);
}

/** The projects whose first message is on its way: a second Enter while one is sending starts nothing. */
const sending = new Set<string>();

/**
 * Send in the empty pane (Enter): the message is the go-ahead (d0087), on `agent` after "New agent on …" (the view's
 * `starts_on`). Starts one agent per press, never a second while the first is on its way, and nothing for a box
 * without words; the pane then shows the new agent with its terminal focused. Resolves to the new agent's id, or
 * `null` when nothing was sent.
 */
export async function sendFirstMessage(
  project: string,
  message: string,
  ask: typeof request,
  routes: PaneRoutes = PANE_ROUTES,
  agent: string | null = null,
  starts: Store<StartOn> = startOn,
): Promise<string | null> {
  if (message.trim() === "" || sending.has(project)) return null;
  sending.add(project);
  try {
    const params: PaneSendParams = { project, message, ...(agent !== null && { agent }) };
    const id = (await ask("pane.send", params)) as string;
    clearStartOn(project, starts);
    showAgent(project, id, routes.shown);
    requestPaneFocus(id, routes.focus);
    return id;
  } finally {
    sending.delete(project);
  }
}

/**
 * Split with the Project agent on an epic with no children (LEAD-13, d0091): the pane is shown and sent Rust's first
 * message ("Split NOTE-10 into tickets") as if typed into its empty box; the click is the go-ahead. The pane then
 * shows the new agent. A refusal is the service's toast (`pane.send`); resolves to the new agent's id, or `null`.
 */
export function splitWithProjectAgent(
  split: LeadSplit,
  ask: typeof request,
  routes: PaneRoutes = PANE_ROUTES,
): Promise<string | null> {
  selectProject(split.project, routes.selection);
  showPane(routes);
  return sendFirstMessage(split.project, split.message, ask, routes).catch(() => null);
}
