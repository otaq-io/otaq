// The Approve flow's steps (UIT-04: which step shows is the page's; every text and rule is Rust's, `review_page.rs`):
// the Approve ▾ choice, "changed since you reviewed it", the Local merge confirmation, Merging…, and its result; or
// "Done, nothing to merge", which finishes the ticket at once.
import type { ApproveChoice } from "../../generated/ApproveChoice";
import type { ApproveView } from "../../generated/ApproveView";
import type { LocalMergeResult } from "../../generated/LocalMergeResult";
import type { ReviewChanged } from "../../generated/ReviewChanged";

export type Step =
  | { kind: "menu" }
  /** `checkedFirst`: found when the dialog opened (Approve anyway goes on to the choice or the confirmation),
   * rather than as a merge's answer (Approve anyway merges at once). */
  | { kind: "changed"; changed: ReviewChanged; checkedFirst: boolean }
  | { kind: "confirm" }
  | { kind: "merging" }
  | { kind: "result"; result: Exclude<LocalMergeResult, { kind: "merged" } | { kind: "changed" }> }
  /** Open PR or Push to PR: T-0227's dialog takes over. */
  | { kind: "open_pr" }
  /** "Done, nothing to merge": `review.done` runs at once (no confirmation), its toasts say how it ended. */
  | { kind: "finish" }
  /** Merge into another ticket…: its picker takes over. */
  | { kind: "merge_into" }
  /** Done, don't merge: its confirmation (what won't land) takes over. */
  | { kind: "done_not_merged" }
  | { kind: "done" };

/** Where the flow starts: a ticket with a PR goes to Push to PR; new commits since the review show first. `choice`
 * (the review bar's Approve or a row of its menu) goes straight to that choice's dialog. */
export function firstStep(view: ApproveView, menu: boolean, choice?: ApproveChoice): Step {
  if (view.changed !== null) return { kind: "changed", changed: view.changed, checkedFirst: true };
  return afterChanged(view, menu, choice);
}

/** The choice, or with one way to deliver its dialog without the choice (dogfood #18), or the confirmation. With
 * nothing to merge the choice always shows: it leads with Done, and Local merge is disabled. */
function afterChanged(view: ApproveView, menu: boolean, choice?: ApproveChoice): Step {
  if (view.push_to_pr) return { kind: "open_pr" };
  if (choice !== undefined) return afterChoice(choice);
  if (leadsWithDone(view)) return { kind: "menu" };
  if (!menu) return { kind: "confirm" };
  return view.menu.direct === null ? { kind: "menu" } : afterChoice(view.menu.direct);
}

/** Whether the menu leads with "Done, nothing to merge" (the branch has no commits beyond its base). */
export function leadsWithDone(view: ApproveView): boolean {
  return view.menu.items[0]?.choice === "done";
}

/** A pick in the Approve ▾ menu. */
export function afterChoice(choice: ApproveChoice): Step {
  switch (choice) {
    case "done":
      return { kind: "finish" };
    case "local_merge":
      return { kind: "confirm" };
    case "merge_into":
      return { kind: "merge_into" };
    case "done_not_merged":
      return { kind: "done_not_merged" };
    case "open_pr":
    case "push_to_pr":
      return { kind: "open_pr" };
  }
}

/** Approve anyway: back to where the flow was going, or (as a merge's answer) the merge again. */
export function afterApproveAnyway(
  step: Extract<Step, { kind: "changed" }>,
  view: ApproveView,
  menu: boolean,
  choice?: ApproveChoice,
): Step | "merge" {
  if (!step.checkedFirst) return "merge";
  return afterChanged(view, menu, choice);
}

/** The prepared message replaces the plain one when it arrives, unless the user typed: the message box then takes
 * `next`'s; otherwise it keeps what it has. */
export function messageAfter(
  before: { writing: string | null } | null,
  next: { writing: string | null; message: string },
  typed: boolean,
): string | null {
  if (typed || before === null || before.writing === null || next.writing !== null) return null;
  return next.message;
}

/** The step a Local merge's answer leads to. */
export function afterMerge(result: LocalMergeResult): Step {
  switch (result.kind) {
    case "merged":
      return { kind: "done" };
    case "changed":
      return { kind: "changed", changed: result.changed, checkedFirst: false };
    case "conflict":
    case "refused":
    case "stopped":
    case "uncommitted":
    case "identity":
      return { kind: "result", result };
  }
}

/** How many of the new commits' files the changed-since-review step shows with their lines. */
export const CHANGED_FILES_SHOWN = 10;
