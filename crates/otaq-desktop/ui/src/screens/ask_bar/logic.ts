// The Ask bar's moment of interaction (T-0458; SHB-02, SHB-03, d0113): the chosen suggestion, Return, "New reply" and
// telling Rust which conversation the panel shows. What the bar says, what it suggests and whether a reply waits are
// Rust's (`ask_bar.view`, `otaq-view`'s `ask_bar.rs`). Nothing here starts an agent without the user's words: Return
// with an empty field sends nothing (d0068, d0087).
import { createStore, type Store } from "../../app/store";
import type { AskBarView } from "../../generated/AskBarView";
import type { AskLookedParams } from "../../generated/AskLookedParams";
import type { AskSendParams } from "../../generated/AskSendParams";
import type { AskSent } from "../../generated/AskSent";
import type { AskSuggestion } from "../../generated/AskSuggestion";
import { request } from "../../tauri";
import {
  AGENT_PANEL_ROUTES,
  type AgentPanelRoutes,
  requestAgentPanelFocus,
  showAgent,
  showAgentPanel,
} from "../agent_panel/logic";

/** The open project's bar, for the rail's Project agent button (its tint and dot); `undefined` before it loads. */
export const askBar: Store<AskBarView | undefined> = createStore<AskBarView | undefined>(undefined);

/** Counts each `ask_bar.looked`: the bar asks again, so a reply the user just saw stops being new. */
export const lookedCount: Store<number> = createStore(0);

const ignore = () => undefined;

/** The panel shows `agent` (or stopped showing it): its replies until now aren't new. */
export function lookedAt(
  agent: string,
  ask: typeof request = request,
  count: Store<number> = lookedCount,
): void {
  const params: AskLookedParams = { agent };
  ask("ask_bar.looked", params).then(() => {
    count.update((n) => n + 1);
  }, ignore);
}

/** The suggestion chosen after an arrow key: `delta` rows on from `index`, wrapping. */
export function nextChoice(index: number, count: number, delta: number): number {
  if (count === 0) return 0;
  return (((index + delta) % count) + count) % count;
}

/** The panel open on `agent` with its terminal focused: after Return, and for "New reply". */
export function openOn(project: string, agent: string, routes: AgentPanelRoutes = AGENT_PANEL_ROUTES): void {
  showAgent(project, agent, routes.shown);
  showAgentPanel(routes);
  requestAgentPanelFocus(agent, routes.focus);
}

/** The words on their way, by project: a second Return while one is sending sends nothing. */
const sending = new Set<string>();

/**
 * Return (SHB-03): sends `text` as `suggestion` asks, then opens the panel on the conversation. Resolves to whether
 * the words went (the bar clears them then), `false` when nothing was sent.
 */
export async function sendFromBar(
  project: string,
  text: string,
  suggestion: AskSuggestion,
  ask: typeof request = request,
  routes: AgentPanelRoutes = AGENT_PANEL_ROUTES,
): Promise<boolean> {
  if (text.trim() === "" || sending.has(project)) return false;
  sending.add(project);
  try {
    const params: AskSendParams = {
      project,
      text,
      about: suggestion.about,
      ...(suggestion.ticket !== null && { ticket: suggestion.ticket }),
      ...(suggestion.agent !== null && { agent: suggestion.agent }),
    };
    const answer = (await ask("ask_bar.send", params)) as AskSent;
    openOn(project, answer.agent, routes);
    return answer.sent;
  } finally {
    sending.delete(project);
  }
}
