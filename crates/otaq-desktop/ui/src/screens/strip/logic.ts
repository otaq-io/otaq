// The Needs you strip's moment of interaction (T-0459; NYS-01, NYS-02, NYS-03; d0114, d0016): what the page shows
// now (which Next moves on from), opening a chip or Next's answer, and a lead's suggestion's Not now and Confirm. What
// the strip says and where Next goes are Rust's (`strip.view`, `strip.next`, `otaq-view`'s `needs_you_strip.rs`).
// Nothing opens without the user's press: Next never moves on by itself (d0016).
import type { SuggestionConfirm } from "../../app/registry";
import {
  focusTerminal,
  openTicket,
  requestTerminalFocus,
  type Selection,
  selection,
  selectProject,
  type TerminalFocusRequest,
  terminalFocusRequests,
} from "../../app/selection";
import { createStore, type Store } from "../../app/store";
import type { StripOpen } from "../../generated/StripOpen";
import type { StripParams } from "../../generated/StripParams";
import type { StripSuggestionParams } from "../../generated/StripSuggestionParams";
import type { StripTarget } from "../../generated/StripTarget";
import { request } from "../../tauri";
import { chosenAgent, PANE_ROUTES, type PaneRoutes, showAgentInPane } from "../pane/logic";

/** Where opening an item lands: the selection, the terminal focus requests, the Project agent panel (its `selection`
 * is the same store), the last item the strip opened, and taking the keyboard out of the focused terminal. */
export interface StripRoutes {
  selection: Store<Selection>;
  focus: Store<TerminalFocusRequest | null>;
  pane: PaneRoutes;
  last: Store<StripTarget | null>;
  /** Takes the keyboard from the element that has it (the focused terminal's). */
  blur: () => void;
}

/** The item the strip (a chip, Next or ⌘J) opened last. */
export const lastOpened: Store<StripTarget | null> = createStore<StripTarget | null>(null);

export const STRIP_ROUTES: StripRoutes = {
  selection,
  focus: terminalFocusRequests,
  pane: PANE_ROUTES,
  last: lastOpened,
  blur: () => {
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  },
};

/**
 * What the page shows now, which Next moves on from: the Project agent the strip opened while the panel still shows
 * it, else the open ticket, else the conversation in the open panel; `null` for none.
 */
export function openOf(routes: StripRoutes = STRIP_ROUTES): StripOpen | null {
  const { project, ticket } = routes.selection.get();
  const agentOpen = routes.pane.layout.get().agentOpen;
  const shown = project === null ? null : chosenAgent(routes.pane.shown.get(), project);
  const last = routes.last.get();
  if (last?.kind === "agent" && agentOpen && shown === last.agent)
    return { kind: "agent", agent: last.agent };
  if (ticket !== null) return { kind: "ticket", key: ticket };
  if (agentOpen && shown !== null) return { kind: "agent", agent: shown };
  return null;
}

/** The params of `strip.view` and `strip.next` for `project`. */
export function stripParams(project: string, open: StripOpen | null): StripParams {
  return open === null ? { project } : { project, open };
}

/**
 * Opens an item (NYS-01): its project on screen; a ticket on its tab with its terminal focused when that is the tab
 * (an epic's Lead tab is its lead's terminal); a Project agent in the panel, focused. A terminal that had the keyboard
 * loses it first, so nothing typed meanwhile reaches the ticket that was open (finding 55): the target's terminal
 * takes it once it opens, and a target with no terminal leaves the keyboard on the page.
 */
export function openTarget(target: StripTarget, routes: StripRoutes = STRIP_ROUTES): void {
  routes.last.set(target);
  if (routes.selection.get().focusedTerminal !== null) {
    routes.blur();
    focusTerminal(null, routes.selection);
  }
  switch (target.kind) {
    case "ticket":
      selectProject(target.project, routes.selection);
      openTicket(target.key, target.tab, routes.selection);
      if (target.tab === "agent" || target.tab === "check" || target.tab === "lead")
        requestTerminalFocus(target.key, routes.focus);
      break;
    case "agent":
      showAgentInPane(target.project, target.agent, routes.pane);
      break;
  }
}

const ignore = () => undefined;

/**
 * ⌘J and Next (NYS-02): asks where Next goes from what the page shows (this project's next item, or the next project
 * with items) and opens it; nothing when nothing waits.
 */
export async function runNext(
  ask: typeof request = request,
  routes: StripRoutes = STRIP_ROUTES,
): Promise<void> {
  const { project } = routes.selection.get();
  if (project === null) return;
  const target = (await ask("strip.next", stripParams(project, openOf(routes)))) as StripTarget | null;
  if (target !== null) openTarget(target, routes);
}

/** ⌘J from anywhere: a refusal opens nothing. */
export function next(): void {
  runNext().catch(ignore);
}

/** A lead's suggestion: Not now dismisses it, the confirmation's confirm (`confirmSuggestion`) carries it out. */
export function dismissSuggestion(id: string, ask: typeof request = request): Promise<unknown> {
  const params: StripSuggestionParams = { id, confirm: false };
  return ask("strip.suggestion", params);
}

/** The confirm of a suggestion's close or delete confirmation: the service closes or deletes the child (LEAD-08). */
export function confirmSuggestion(
  suggestion: SuggestionConfirm,
  ask: typeof request = request,
): Promise<unknown> {
  const params: StripSuggestionParams = { id: suggestion.id, confirm: true };
  return ask("strip.suggestion", params);
}
