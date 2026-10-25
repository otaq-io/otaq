// Session's terminal's own decisions (SHT-04), as plain functions the tests drive without a terminal: which session a
// side attaches to, which body, which strip (state label and hint) fits the terminal's phase and focus, whether the
// sheet is narrow, and whether a terminal focus request is for it. Every text comes from `terminal_tab.view` (Rust).
import type { TerminalFocusRequest } from "../../app/selection";
import { createStore, type Store } from "../../app/store";
import type { TerminalPhase } from "../../generated/TerminalPhase";
import type { TerminalStripTone } from "../../generated/TerminalStripTone";
import type { TerminalTabCheck } from "../../generated/TerminalTabCheck";
import type { TerminalTabStrip } from "../../generated/TerminalTabStrip";
import type { TerminalTabView } from "../../generated/TerminalTabView";

/** What fills the terminal's place under its strip. */
export type TabBody = "terminal" | "recovery" | "no_session";

/** Session's sides with a terminal of the ticket's: its agent's and its check's (SHT-04). */
export type Side = "agent" | "check";

/**
 * The check side shown: the check agent's (`agent`, the switch's part, d0130: one per check agent), else the first;
 * `null` before a check ran.
 */
export function checkOf(view: TerminalTabView, agent: string | null = null): TerminalTabCheck | null {
  return view.checks.find((c) => c.agent === agent) ?? view.checks[0] ?? null;
}

/**
 * Which check agent's side Session shows (d0130: one per check agent): its name as the switch's part has it, `null`
 * for the first. The page's own state; a part's click or a review block's Open terminal sets it.
 */
export const shownCheckAgent: Store<string | null> = createStore<string | null>(null);

/** Whether the switch's part is the one shown. */
export function partShown(
  part: { content: string; agent: string | null },
  shown: string,
  agent: string | null,
  parts: readonly { content: string; agent: string | null }[],
): boolean {
  if (part.content !== shown) return false;
  if (part.content !== "check" || part.agent === null) return true;
  const known = parts.some((p) => p.content === "check" && p.agent === agent);
  const first = parts.find((p) => p.content === "check");
  return known ? part.agent === agent : first === part;
}

/** The session the terminal attaches to on that side. */
export function sessionOf(view: TerminalTabView, side: Side, agent: string | null = null): string | null {
  const check = side === "check" ? checkOf(view, agent) : null;
  return check !== null ? check.session : view.session;
}

export function bodyOf(view: TerminalTabView, side: Side = "agent", agent: string | null = null): TabBody {
  if (side === "check" && checkOf(view, agent) !== null) return "terminal";
  if (view.recovery !== null) return "recovery";
  return view.session === null ? "no_session" : "terminal";
}

/** The terminal's phase as the strip shows it: a ticket without a session is `no_session` whatever it was told. */
export function shownPhase(
  view: TerminalTabView,
  phase: TerminalPhase,
  side: Side = "agent",
  agent: string | null = null,
): TerminalPhase {
  return bodyOf(view, side, agent) === "terminal" ? phase : "no_session";
}

/** The state label and hint for this phase and focus. */
export function stripFor(
  view: TerminalTabView,
  phase: TerminalPhase,
  focused: boolean,
  side: Side = "agent",
  agent: string | null = null,
): TerminalTabStrip | undefined {
  const shown = shownPhase(view, phase, side, agent);
  const check = side === "check" ? checkOf(view, agent) : null;
  const strips = check !== null ? check.strips : view.strips;
  return strips.find((s) => s.phase === shown && s.focused === focused);
}

/** The CSS colour of a state label. */
export function toneColor(tone: TerminalStripTone): string {
  switch (tone) {
    case "working":
      return "var(--st-working)";
    case "attention":
      return "var(--st-attn)";
    case "secondary":
      return "var(--text-secondary)";
    case "muted":
      return "var(--text-muted)";
    case "error":
      return "var(--st-error)";
  }
}

/** A narrow sheet: the strip drops the port and shortens the attach command. */
export function isNarrow(width: number, view: TerminalTabView): boolean {
  return width > 0 && width < view.narrow_below;
}

/** A focus request this terminal answers: for its ticket and newer than the last one answered. */
export function answers(request: TerminalFocusRequest | null, ticket: string, answered: number): boolean {
  return request !== null && request.ticket === ticket && request.seq > answered;
}

/** "Scrolled up 1,320 lines": the hint while the terminal is scrolled up (a count only the page knows). */
export function scrolledLabel(lines: number): string {
  return lines === 1 ? "Scrolled up 1 line" : `Scrolled up ${lines.toLocaleString("en-US")} lines`;
}
