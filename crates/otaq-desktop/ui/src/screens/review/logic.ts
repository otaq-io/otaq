// The Approve flow's steps (UIT-04: which step shows is the page's; every text and rule is Rust's, `review_page.rs`):
// the Approve ▾ choice, "changed since you reviewed it", the Local merge confirmation, Merging…, and its result.
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
  | { kind: "done" };

/** Where the flow starts: a ticket with a PR goes to Push to PR; new commits since the review show first. */
export function firstStep(view: ApproveView, menu: boolean): Step {
  if (view.changed !== null) return { kind: "changed", changed: view.changed, checkedFirst: true };
  if (view.push_to_pr) return { kind: "open_pr" };
  return menu ? { kind: "menu" } : { kind: "confirm" };
}

/** A pick in the Approve ▾ menu. */
export function afterChoice(choice: ApproveChoice): Step {
  return choice === "local_merge" ? { kind: "confirm" } : { kind: "open_pr" };
}

/** Approve anyway: back to where the flow was going, or (as a merge's answer) the merge again. */
export function afterApproveAnyway(
  step: Extract<Step, { kind: "changed" }>,
  view: ApproveView,
  menu: boolean,
): Step | "merge" {
  if (!step.checkedFirst) return "merge";
  if (view.push_to_pr) return { kind: "open_pr" };
  return menu ? { kind: "menu" } : { kind: "confirm" };
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
      return { kind: "result", result };
  }
}

/** ⌘↵ / Ctrl+↵ in the message presses Squash & merge. */
export function isSubmitKey(e: { key: string; metaKey: boolean; ctrlKey: boolean }): boolean {
  return e.key === "Enter" && (e.metaKey || e.ctrlKey);
}

/** How many of the new commits' files the changed-since-review step shows with their lines. */
export const CHANGED_FILES_SHOWN = 10;
