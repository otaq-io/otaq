// The Project agent panel's moment of interaction (T-0330, T-0458; SHL-04, SHB-03, SHB-04, d0087, d0113): which
// conversation each project shows, opening and closing the panel, the requests to give it the keyboard (⌘L, ⇧⌘L, an
// strip item, the Ask bar), and Send's guard against a second start. Everything the panel says, and where a project
// opens the first time (`opens_on`), is Rust's (`agent_panel.view`, `otaq-view`'s `agent_panel.rs`). Nothing here starts an
// agent except Send with words, the user's go-ahead (d0087, d0018). The code calls the panel "the agent panel", as before.
import { setAgentExpanded, setAgentOpen, type ShellLayout, shellLayout } from "../../app/layout";
import { type Selection, selectProject, selection } from "../../app/selection";
import { createStore, type Store } from "../../app/store";
import type { AgentPanelButton } from "../../generated/AgentPanelButton";
import type { AgentPanelSendParams } from "../../generated/AgentPanelSendParams";
import type { MenuItem } from "../../generated/MenuItem";
import type { request } from "../../tauri";

/**
 * What each project shows, by project key: an agent's id, or `null` for the empty agent panel ("+", ⇧⌘L). A project not in
 * the map has no choice yet and opens on the view's `opens_on`.
 */
export type ShownAgents = Readonly<Record<string, string | null>>;

export const shownAgents: Store<ShownAgents> = createStore<ShownAgents>({});

/** The agent `agent_panel.view` is asked for: the choice for `project`, else none (the view then says where it opens). */
export function chosenAgent(shown: ShownAgents, project: string): string | null {
  return shown[project] ?? null;
}

/** Whether the page chose for `project` yet (the empty agent panel is a choice too). */
export function hasChoice(shown: ShownAgents, project: string): boolean {
  return project in shown;
}

/** Shows `agent` in `project`'s agent panel, or the empty agent panel for `null`. */
export function showAgent(
  project: string,
  agent: string | null,
  store: Store<ShownAgents> = shownAgents,
): void {
  store.update((s) => (hasChoice(s, project) && s[project] === agent ? s : { ...s, [project]: agent }));
}

/**
 * The named agent the empty agent panel starts on after "New agent on …" (M23, SHL-04), by project key: `agent_panel.view`'s
 * `start_on` and `agent_panel.send`'s `agent`. A project not in the map starts on its Project agent default.
 */
export type StartOn = Readonly<Record<string, string>>;

export const startOn: Store<StartOn> = createStore<StartOn>({});

/** Drops `project`'s "New agent on …" choice: the empty agent panel starts on the Project agent default again. */
export function clearStartOn(project: string, store: Store<StartOn> = startOn): void {
  store.update((s) => {
    if (!(project in s)) return s;
    return Object.fromEntries(Object.entries(s).filter(([key]) => key !== project));
  });
}

/** A request to give the agent panel the keyboard: the shown agent's terminal, or the empty agent panel's box. */
export interface AgentPanelFocusRequest {
  /** Counts up with each request, so asking twice is two requests. */
  seq: number;
  /** Wait until the agent panel shows this agent (`null`: the empty agent panel) before focusing; absent: whatever it shows. */
  agent?: string | null;
}

export const agentPanelFocusRequests: Store<AgentPanelFocusRequest | null> =
  createStore<AgentPanelFocusRequest | null>(null);

export function requestAgentPanelFocus(
  agent?: string | null,
  store: Store<AgentPanelFocusRequest | null> = agentPanelFocusRequests,
): void {
  store.update((last) => ({ seq: (last?.seq ?? 0) + 1, ...(agent !== undefined && { agent }) }));
}

/** Whether a focus request is for what the agent panel shows now. */
export function focusFits(asked: AgentPanelFocusRequest, shown: string | null): boolean {
  return asked.agent === undefined || asked.agent === shown;
}

/** What the agent panel gives the keyboard to: the shown agent's terminal while it has a session, else the box (if any). */
export function focusTarget(view: {
  running: { session: string | null } | null;
}): "terminal" | "box" | "none" {
  if (view.running === null) return "box";
  return view.running.session !== null ? "terminal" : "none";
}

/** Where the agent panel's keys and routes land; the tests pass their own stores. */
export interface AgentPanelRoutes {
  selection: Store<Selection>;
  layout: Store<ShellLayout>;
  shown: Store<ShownAgents>;
  focus: Store<AgentPanelFocusRequest | null>;
}

export const AGENT_PANEL_ROUTES: AgentPanelRoutes = {
  selection,
  layout: shellLayout,
  shown: shownAgents,
  focus: agentPanelFocusRequests,
};

/** Opens the panel (d0113); the rest of the layout stays. */
export function showAgentPanel(routes: AgentPanelRoutes = AGENT_PANEL_ROUTES): void {
  setAgentOpen(true, routes.layout);
}

/** Folds the panel back into the Ask bar: its close icon, ⌘L from inside it, Esc (d0113). The conversation is kept. */
export function closeAgentPanel(routes: AgentPanelRoutes = AGENT_PANEL_ROUTES): void {
  setAgentOpen(false, routes.layout);
}

/** The header's Expand and its way back (T-0502): the open panel over the board at full width, or beside it again. */
export function toggleExpandAgentPanel(routes: AgentPanelRoutes = AGENT_PANEL_ROUTES): void {
  setAgentExpanded(!routes.layout.get().agentExpanded, routes.layout);
}

/**
 * Esc while the panel is expanded (T-0502), wherever the keyboard is outside a terminal (in one, Esc is the agent's):
 * back beside the board, still open. Whether it took the key.
 */
export function leaveExpand(routes: AgentPanelRoutes = AGENT_PANEL_ROUTES): boolean {
  if (!routes.layout.get().agentExpanded) return false;
  setAgentExpanded(false, routes.layout);
  return true;
}

/**
 * ⌘L (SHB-04): opens the panel and gives the keyboard to the shown conversation's terminal or the empty panel's box;
 * pressed while the panel has the keyboard (`inPanel`), it closes it.
 */
export function focusAgentPanel(routes: AgentPanelRoutes = AGENT_PANEL_ROUTES, inPanel = false): boolean {
  if (routes.selection.get().project === null) return false;
  if (routes.layout.get().agentOpen && inPanel) {
    closeAgentPanel(routes);
    return true;
  }
  showAgentPanel(routes);
  requestAgentPanelFocus(undefined, routes.focus);
  return true;
}

/** Whether `element` is in the Project agent panel (the frame's `agent_panel` slot). */
export function inPanel(element: Element | null): boolean {
  return element?.closest('[data-slot="agent_panel"]') != null;
}

/** The rail's Project agent button: opens the panel with the keyboard in it, or closes an open one. */
export function toggleAgentPanel(routes: AgentPanelRoutes = AGENT_PANEL_ROUTES): boolean {
  if (routes.layout.get().agentOpen) {
    closeAgentPanel(routes);
    return true;
  }
  return focusAgentPanel(routes);
}

/** ⇧⌘L and +: the empty agent panel, its box focused. Nothing is asked of the service, so nothing starts (d0087). */
export function newProjectAgent(
  routes: AgentPanelRoutes = AGENT_PANEL_ROUTES,
  starts: Store<StartOn> = startOn,
): boolean {
  const { project } = routes.selection.get();
  if (project === null) return false;
  clearStartOn(project, starts);
  showAgent(project, null, routes.shown);
  showAgentPanel(routes);
  requestAgentPanelFocus(null, routes.focus);
  return true;
}

/**
 * A strip item, an OS notification of a Project agent, or the Ask bar's "New reply": its project on screen, the agent
 * in the panel (opened) and its terminal focused (PAG-07, ATT-03: the focus clears its wait).
 */
export function showAgentInPanel(
  project: string,
  agent: string,
  routes: AgentPanelRoutes = AGENT_PANEL_ROUTES,
): void {
  selectProject(project, routes.selection);
  showAgent(project, agent, routes.shown);
  showAgentPanel(routes);
  requestAgentPanelFocus(agent, routes.focus);
}

/**
 * "New agent on Sonnet" in a notice (SHL-04, M23): the empty agent panel on that named agent, its box focused. Nothing
 * starts until the user sends (d0087); the first message then starts it on `agent`.
 */
export function startNewAgentOn(
  project: string,
  agent: string,
  routes: AgentPanelRoutes = AGENT_PANEL_ROUTES,
  starts: Store<StartOn> = startOn,
): void {
  starts.update((s) => (s[project] === agent ? s : { ...s, [project]: agent }));
  showAgent(project, null, routes.shown);
  showAgentPanel(routes);
  requestAgentPanelFocus(null, routes.focus);
}

/** The projects whose first message is on its way: a second Enter while one is sending starts nothing. */
const sending = new Set<string>();

/**
 * Send in the empty agent panel (Enter): the message is the go-ahead (d0087), on `agent` after "New agent on …" (the view's
 * `starts_on`). Starts one agent per press, never a second while the first is on its way, and nothing for a box
 * without words; the agent panel then shows the new agent with its terminal focused. Resolves to the new agent's id, or
 * `null` when nothing was sent.
 */
export async function sendFirstMessage(
  project: string,
  message: string,
  ask: typeof request,
  routes: AgentPanelRoutes = AGENT_PANEL_ROUTES,
  agent: string | null = null,
  starts: Store<StartOn> = startOn,
): Promise<string | null> {
  if (message.trim() === "" || sending.has(project)) return null;
  sending.add(project);
  try {
    const params: AgentPanelSendParams = { project, message, ...(agent !== null && { agent }) };
    const id = (await ask("agent_panel.send", params)) as string;
    clearStartOn(project, starts);
    showAgent(project, id, routes.shown);
    requestAgentPanelFocus(id, routes.focus);
    return id;
  } finally {
    sending.delete(project);
  }
}

/** The … menu's id for "New conversation" while the header has folded + into it (not one of `agent_panel.rs`'s ids). */
export const NEW_ITEM = "new";

/**
 * The … menu as it opens. A narrow panel folds the header's Recently ended and + into it (the container query in
 * `agent_panel.css`): Rust's menu already ends with Recently ended…, so only + joins it, first, worded as the header's
 * button and only while a conversation is shown (the empty panel has no +).
 */
export function moreMenu(
  menu: readonly MenuItem[],
  newButton: AgentPanelButton,
  folded: boolean,
  running: boolean,
): readonly MenuItem[] {
  if (!folded || !running) return menu;
  const item: MenuItem = {
    id: NEW_ITEM,
    label: newButton.label,
    icon: newButton.icon,
    shortcut: newButton.keys,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: false,
    header: false,
  };
  return [item, ...menu.map((row, i) => (i === 0 ? { ...row, separated: true } : row))];
}

/** Whether the narrow header folded `button` away (`display: none` on its tooltip's anchor leaves it no boxes). */
export function foldedAway(button: Element | null): boolean {
  return button !== null && button.getClientRects().length === 0;
}
