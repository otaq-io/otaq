// Where the ticket's buttons go (T-0222), in its sheet (T-0464). What the buttons and menus say is Rust's
// (`src/api/sheet.rs`); this file keeps only the moment of interaction: the picked answer, and which buttons open a
// tab or a dialog on the page instead of calling the service.
import type { CardActionKind } from "../../generated/CardActionKind";
import type { SheetAction } from "../../generated/SheetAction";
import type { SheetAnswerRow } from "../../generated/SheetAnswerRow";

/** Where a button goes: the service (`sheet.action`), a tab, or a dialog. */
export type ActionRoute =
  | { kind: "service" }
  // Start: `board.start`, into the first stage (where it waits at the top until it can begin).
  | { kind: "start" }
  | { kind: "terminal" }
  // Send again (d0118): `sheet.send_again`, whose answer may be the unsent-text question.
  | { kind: "send_again" }
  | { kind: "check" }
  | { kind: "answer_agent" }
  | { kind: "diff" }
  | { kind: "edit" }
  | { kind: "parent" }
  | { kind: "overlay"; id: "local_merge" | "open_pr" | "send_back" | "send_back_pr" | "start_from_scratch" }
  // Pass (FLW-14): `review.pass`, then the delivery choices when a click Delivery comes next.
  | { kind: "pass" }
  | { kind: "mark_closed" }
  | { kind: "run" }
  // Open agent… (AGM-03, d0108 second gap review rule 7): Settings → Agents on the sheet's `open_agent`.
  | { kind: "open_agent" };

// No `default`: a new action fails ESLint's switch-exhaustiveness-check until it gets its route.
export function actionRoute(action: SheetAction): ActionRoute {
  switch (action) {
    case "start":
      return { kind: "start" };
    case "retry_start":
    case "resume":
    case "retry_resume":
    case "reopen":
    case "mark_merged":
    case "open_pr_in_browser":
    case "move_on_anyway":
    case "another_round":
    case "retry_validation":
    case "retry_merge":
    case "move_to_review":
    case "stop_merge_check": // d0120: on a connection of its own (Rust's), the merge finishes without merging.
    case "stop_git_step": // A merge's or a start's git step and its hook end; on a connection of its own (Rust's).
      return { kind: "service" };
    // Session on the check's side (SHT-04).
    case "open_validator":
    case "answer_validator":
      return { kind: "check" };
    // Opens the note box under the bar; Send to agent calls `sheet.answer_agent`.
    case "answer_agent":
      return { kind: "answer_agent" };
    case "open_terminal":
    case "answer_in_terminal":
      return { kind: "terminal" };
    case "send_again":
      return { kind: "send_again" };
    case "view_diff":
      return { kind: "diff" };
    case "edit":
      return { kind: "edit" };
    case "view_parent":
      return { kind: "parent" };
    // Edit message and retry (a hook refused Approve & merge): its confirmation again, on the message the user edited.
    case "approve":
    case "edit_message":
      return { kind: "overlay", id: "local_merge" };
    // Edit messages and retry (a hook refused a PR's commit message): Approve & open PR again, on the edited messages.
    case "edit_pr_messages":
      return { kind: "overlay", id: "open_pr" };
    case "send_back":
      return { kind: "overlay", id: "send_back" };
    case "pass":
      return { kind: "pass" };
    case "send_back_pr":
      return { kind: "overlay", id: "send_back_pr" };
    case "start_from_scratch":
      return { kind: "overlay", id: "start_from_scratch" };
    case "mark_closed":
      return { kind: "mark_closed" };
    case "run":
      return { kind: "run" };
    case "open_agent":
      return { kind: "open_agent" };
  }
}

/** Settings → Agents on `agent`'s page (Open agent…), or on the list when the agent isn't known. */
export function agentSettings(agent: string | null): { section: "agents"; agent?: string } {
  return agent === null ? { section: "agents" } : { section: "agents", agent };
}

/** Where a "…" menu item goes; ids are `sheet_actions.rs`'s, `close.rs`'s, `card_menu.rs`'s and `delete.rs`'s. */
export type MenuRoute =
  | { kind: "edit" }
  | { kind: "service" }
  // Copy branch name.
  | { kind: "copy" }
  // Ask Noyan about this: Noyan's panel on a new conversation addressed to the ticket; nothing starts before words.
  | { kind: "ask_noyan" }
  | { kind: "overlay"; id: "close_ticket" | "submit_for_review" | "delete_ticket" }
  | { kind: "unknown" };

/** The menu's Close ticket… item (`close.rs`'s `CLOSE_TICKET`). */
export const CLOSE_TICKET = "close-ticket";

export function menuRoute(item: string): MenuRoute {
  switch (item) {
    case "edit-ticket":
      return { kind: "edit" };
    case "pin":
    case "unpin":
    case "stop-validation":
    case "skip-to-review":
    case "validate-and-submit":
      return { kind: "service" };
    case "copy-branch":
      return { kind: "copy" };
    case "ask-noyan":
      return { kind: "ask_noyan" };
    case CLOSE_TICKET:
      return { kind: "overlay", id: "close_ticket" };
    case "submit-for-review":
      return { kind: "overlay", id: "submit_for_review" };
    case "delete":
      return { kind: "overlay", id: "delete_ticket" };
    default:
      return { kind: "unknown" };
  }
}

/**
 * Where a board card's button goes once the sheet shows its ticket (T-0220 leaves it in `cardActions`), as the
 * GPUI app's card buttons did: the service (`sheet.card`), the answer list (an option picked, or Send answer with
 * the recommended one), or a dialog. Review only opens the sheet; the Run chip shows the Run tab.
 */
export type CardRoute =
  | { kind: "service" }
  // A Backlog card's Start: `board.start`, as the sheet's Start.
  | { kind: "start" }
  | { kind: "pick"; number: number }
  | { kind: "send_answer" }
  | {
      kind: "overlay";
      id: "start_from_scratch" | "open_pr" | "local_merge" | "send_back" | "send_back_pr" | "change_agent";
    }
  // Pass (FLW-14): `review.pass`, then the delivery choices when a click Delivery comes next.
  | { kind: "pass" }
  | { kind: "mark_closed" }
  | { kind: "run_tab" }
  | { kind: "open_agent" }
  | { kind: "provider_settings" }
  | { kind: "none" };

export function cardRoute(kind: CardActionKind): CardRoute {
  if (typeof kind === "object") return { kind: "pick", number: kind.pick_option };
  switch (kind) {
    case "start":
      return { kind: "start" };
    case "deliver_now":
    case "resume":
    case "retry_resume":
    case "retry_start":
    case "retry":
    case "reopen":
    case "mark_merged":
    case "retry_validation":
    case "retry_merge":
    case "move_to_review":
    case "move_on_anyway": // FLW-13: after the last failed round.
    case "skip_check": // FLW-13: a check at the plan limit.
    case "stop_merge_check": // d0120: Stop on "Merging · running the merge check".
    case "stop_git_step": // Stop on "Merging · running your pre-commit hook", "Starting · making its worktree".
    case "check_again": // A stage held for its skill: the finder is asked again.
      return { kind: "service" };
    case "pass":
      return { kind: "pass" };
    // FLW-14 / FLW-15 / FLW-16: Send back (a human check's Fail, d0093), a To deliver card's click, Send back with
    // PR comments.
    case "fail":
      return { kind: "overlay", id: "send_back" };
    case "local_merge":
      return { kind: "overlay", id: "local_merge" };
    case "open_pr":
      return { kind: "overlay", id: "open_pr" };
    // An automatic PR with no PR provider: app Settings, Git & pull requests, where providers are set up.
    case "set_up_provider":
      return { kind: "provider_settings" };
    case "send_back_pr":
      return { kind: "overlay", id: "send_back_pr" };
    case "send_answer":
      return { kind: "send_answer" };
    case "start_from_scratch":
      return { kind: "overlay", id: "start_from_scratch" };
    // REV-14: Push to PR opens the PR dialog (T-0227).
    case "push_to_pr":
      return { kind: "overlay", id: "open_pr" };
    case "mark_closed":
      return { kind: "mark_closed" };
    // RUN-01: the `▶ :4020` chip shows the Run tab (its Open and Stop).
    case "run_chip":
      return { kind: "run_tab" };
    // AGM-03: Open agent… opens Settings → Agents on the agent that can't run (the sheet's `open_agent`).
    case "open_agent":
      return { kind: "open_agent" };
    // Change agent…: the dialog that picks another agent for the stage waiting for its tool.
    case "change_agent":
      return { kind: "overlay", id: "change_agent" };
    // Answer, Reply and Open (Ticket Card v2): the sheet the click opens is where it is dealt with.
    case "open":
    case "review":
    case "decide": // BRD-03: a check out of rounds; the choices are in the ticket (T-0460).
      return { kind: "none" };
  }
}

/** The answer picked when the list opens: the recommended option (Rust marks it picked), else nothing. */
export function initialPick(rows: readonly SheetAnswerRow[]): number | null {
  return rows.find((r) => r.picked)?.number ?? null;
}

/** A number key on the list: the option with that number, if there is one. */
export function pickByKey(key: string, rows: readonly SheetAnswerRow[]): number | null {
  const n = Number(key);
  return Number.isInteger(n) && rows.some((r) => r.number === n) ? n : null;
}

/** Whether the picked row is Other…, which needs typed text before Send is enabled. */
export function picksOther(pick: number | null, rows: readonly SheetAnswerRow[]): boolean {
  return rows.some((r) => r.other && r.number === pick);
}

/** Send is enabled once something is picked, and Other… has text (Rust checks it again: `answer_options.rs`). */
export function canSend(pick: number | null, other: string, rows: readonly SheetAnswerRow[]): boolean {
  if (pick === null) return false;
  return !picksOther(pick, rows) || other.trim() !== "";
}
