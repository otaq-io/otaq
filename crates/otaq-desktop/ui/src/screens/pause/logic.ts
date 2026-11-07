// Pause's interaction rules (UIT-04: only the moment of interaction lives here; the button, the menu's rows, their
// counts and every word come from Rust, `pause.view`): whether the menu is open, its highlight and its keys, and what
// a pick sends. A pick pauses or resumes at once: there is no confirmation (d0112).
import { createStore } from "../../app/store";
import type { PauseRunParams } from "../../generated/PauseRunParams";
import type { PauseScopeRow } from "../../generated/PauseScopeRow";
import type { PauseView } from "../../generated/PauseView";

/** The service events that change the button or the menu's counts. */
export const PAUSE_EVENTS = [
  "pause_changed",
  "projects_changed",
  "ticket_changed",
  "session_state",
  // The user's key for it (KEY-08).
  "settings_changed",
] as const;

/** Whether the scope menu is open: the button's click and ⇧⌘. toggle it. */
export const pauseMenu = createStore(false);

export function togglePauseMenu(): void {
  pauseMenu.set(!pauseMenu.get());
}

export function closePauseMenu(): void {
  pauseMenu.set(false);
}

/** What a key in the open menu does: ↑ ↓ move the highlight (This project first), ↵ picks it, Esc closes. */
export type MenuStep = { kind: "move"; index: number } | { kind: "pick"; index: number } | { kind: "pass" };

export function menuKey(key: string, index: number, count: number): MenuStep {
  if (count === 0) return { kind: "pass" };
  switch (key) {
    case "ArrowDown":
      return { kind: "move", index: (index + 1) % count };
    case "ArrowUp":
      return { kind: "move", index: (index - 1 + count) % count };
    case "Enter":
    case " ":
      return { kind: "pick", index };
    default:
      return { kind: "pass" };
  }
}

/** What picking `row` sends (`pause.run`): its scope and the open project for its own row; the menu only pauses. */
export function runParams(row: PauseScopeRow, view: PauseView): PauseRunParams {
  const params: PauseRunParams = { scope: row.scope, resume: false };
  if (view.project !== null) params.project = view.project;
  return params;
}
