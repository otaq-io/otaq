// The sections' interaction state (T-0223): the picked epic in Move to epic…, and
// which question's reply box is open. What they show comes from Rust (`src/api/sections.rs`).
import type { MoveChoiceView } from "../../generated/MoveChoiceView";
import type { MoveDialogView } from "../../generated/MoveDialogView";

/** The service events that change a section or Activity. */
export const SECTION_EVENTS = [
  "ticket_changed",
  "ticket_deleted",
  "session_state",
  "usage_changed",
  "messages_waiting",
  "message_delivered",
  "needs_you_changed",
  // The epic lead's answers, pass-ons and notes (T-0290).
  "lead_changed",
] as const;

/** The epic picked in Move to epic…: the clicked one, else the first. */
export function pickedChoice(view: MoveDialogView, picked: string | null): MoveChoiceView | undefined {
  return view.choices.find((c) => c.key === picked) ?? view.choices[0];
}

/** Move to epic… can confirm once an epic is picked and nothing refuses. */
export function canMove(view: MoveDialogView, picked: string | null): boolean {
  return view.refusal === null && pickedChoice(view, picked) !== undefined;
}

/** After a move, the sheet follows the ticket to its new key if it showed the old one. */
export function followMove(shown: string | null, oldKey: string, newKey: string): string | null {
  return shown === oldKey ? newKey : null;
}
