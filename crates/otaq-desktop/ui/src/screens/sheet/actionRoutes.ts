// Where the ticket's buttons go (T-0222), in its sheet (T-0464). What the buttons and menus say is Rust's
// (`src/api/panel.rs`); this file keeps only the moment of interaction: the picked answer, and which buttons open a
// tab or a dialog on the page instead of calling the service.
import type { CardActionKind } from "../../generated/CardActionKind";
import type { PanelAction } from "../../generated/PanelAction";
import type { PanelAnswerRow } from "../../generated/PanelAnswerRow";

/** Where a button goes: the service (`panel.action`), a tab, or a dialog. */
export type ActionRoute =
  | { kind: "service" }
  // Start (DPV-03): `board.start`, whose answer may be the blocked ticket's dialog.
  | { kind: "start" }
  | { kind: "terminal" }
  // Send again (d0118): `panel.send_again`, whose answer may be the unsent-text question.
  | { kind: "send_again" }
  | { kind: "check" }
  | { kind: "answer_agent" }
  | { kind: "diff" }
  | { kind: "edit" }
  | { kind: "parent" }
  | { kind: "overlay"; id: "local_merge" | "send_back" | "fail" | "send_back_pr" | "start_from_scratch" }
  // Pass (FLW-14): `review.pass`, then the delivery choices when a click Delivery comes next.
  | { kind: "pass" }
  | { kind: "mark_closed" }
  | { kind: "run" }
  // Open agent… (AGM-03, d0108 second gap review rule 7): Settings → Agents on the panel's `open_agent`.
  | { kind: "open_agent" };

// No `default`: a new action fails ESLint's switch-exhaustiveness-check until it gets its route.
export function actionRoute(action: PanelAction): ActionRoute {
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
    case "resume_lead":
    case "add_child_from_pr":
    case "stop_merge_check": // d0120: on a connection of its own (Rust's), the merge finishes without merging.
      return { kind: "service" };
    // Session on the check's side (SHT-04).
    case "open_validator":
    case "answer_validator":
      return { kind: "check" };
    // Opens the note box under the bar; Send to agent calls `panel.answer_agent`.
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
    case "approve":
      return { kind: "overlay", id: "local_merge" };
    case "send_back":
      return { kind: "overlay", id: "send_back" };
    case "pass":
      return { kind: "pass" };
    case "fail":
      return { kind: "overlay", id: "fail" };
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

/** Where a "…" menu item goes; ids are `panel_actions.rs`'s, `close.rs`'s, `card_menu.rs`'s and `delete.rs`'s. */
export type MenuRoute =
  | { kind: "edit" }
  | { kind: "service" }
  | { kind: "copy"; what: "branch" | "attach" }
  | { kind: "close_epic" }
  | {
      kind: "overlay";
      id: "close_ticket" | "submit_for_review" | "delete_ticket" | "move_to_epic";
    }
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
      return { kind: "copy", what: "branch" };
    case "copy-attach":
      return { kind: "copy", what: "attach" };
    case CLOSE_TICKET:
      return { kind: "overlay", id: "close_ticket" };
    case "submit-for-review":
      return { kind: "overlay", id: "submit_for_review" };
    case "delete":
      return { kind: "overlay", id: "delete_ticket" };
    case "move-to-epic":
      return { kind: "overlay", id: "move_to_epic" };
    case "close-epic":
      return { kind: "close_epic" };
    default:
      return { kind: "unknown" };
  }
}

/**
 * Where a board card's button goes once the panel shows its ticket (T-0220 leaves it in `cardActions`), as the
 * GPUI app's card buttons did: the service (`panel.card`), the answer list (an option picked, or Send answer with
 * the recommended one), or a dialog. Review only opens the panel; the Run chip shows the Run tab.
 */
export type CardRoute =
  | { kind: "service" }
  | { kind: "pick"; number: number }
  | { kind: "send_answer" }
  | { kind: "overlay"; id: "start_from_scratch" | "open_pr" | "local_merge" | "fail" | "send_back_pr" }
  // Pass (FLW-14): `review.pass`, then the delivery choices when a click Delivery comes next.
  | { kind: "pass" }
  | { kind: "mark_closed" }
  | { kind: "run_tab" }
  | { kind: "open_agent" }
  | { kind: "none" };

export function cardRoute(kind: CardActionKind): CardRoute {
  if (typeof kind === "object") return { kind: "pick", number: kind.pick_option };
  switch (kind) {
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
    case "use_fallback": // AGT-08: a held role's fallback takes it (`ticket.use_fallback`, T-0362).
    case "use_validator_fallback":
    case "move_on_anyway": // FLW-13: after the last failed round.
    case "skip_check": // FLW-13: a check at the plan limit with no fallback.
    case "add_child_from_pr": // FLW-16: an epic's PR comments as a child.
    case "stop_merge_check": // d0120: Stop on "Merging · running the merge check".
      return { kind: "service" };
    case "pass":
      return { kind: "pass" };
    // FLW-14 / FLW-15 / FLW-16: Fail, a To deliver card's click, Send back with PR comments.
    case "fail":
      return { kind: "overlay", id: "fail" };
    case "local_merge":
      return { kind: "overlay", id: "local_merge" };
    case "open_pr":
      return { kind: "overlay", id: "open_pr" };
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
    // AGM-03: Open agent… opens Settings → Agents on the agent that can't run (the panel's `open_agent`).
    case "open_agent":
      return { kind: "open_agent" };
    // Answer, Reply and Open (Ticket Card v2): the panel the click opens is where it is dealt with.
    case "open":
    case "review":
    case "decide": // BRD-03: a check out of rounds; the choices are in the ticket (T-0460).
      return { kind: "none" };
  }
}

/** The answer picked when the list opens: the recommended option (Rust marks it picked), else nothing. */
export function initialPick(rows: readonly PanelAnswerRow[]): number | null {
  return rows.find((r) => r.picked)?.number ?? null;
}

/** A number key on the list: the option with that number, if there is one. */
export function pickByKey(key: string, rows: readonly PanelAnswerRow[]): number | null {
  const n = Number(key);
  return Number.isInteger(n) && rows.some((r) => r.number === n) ? n : null;
}

/** Whether the picked row is Other…, which needs typed text before Send is enabled. */
export function picksOther(pick: number | null, rows: readonly PanelAnswerRow[]): boolean {
  return rows.some((r) => r.other && r.number === pick);
}

/** Send is enabled once something is picked, and Other… has text (Rust checks it again: `answer_options.rs`). */
export function canSend(pick: number | null, other: string, rows: readonly PanelAnswerRow[]): boolean {
  if (pick === null) return false;
  return !picksOther(pick, rows) || other.trim() !== "";
}
