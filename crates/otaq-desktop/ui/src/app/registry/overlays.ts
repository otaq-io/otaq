// Every overlay of the page (dialogs, popovers, drawers), fixed here so screens open one another's by id
// (`openOverlay("activity")`) without editing a shared list. Each overlay's own screen registers its component; opening
// one that isn't registered yet does nothing visible. Menus and tooltips tied to one element stay local to their
// component (they may portal into the overlay host, `OVERLAY_HOST_ID`).
import type { ComponentType } from "react";

import type { AgentToolSignIn } from "../../generated/AgentToolSignIn";
import type { ApproveChoice } from "../../generated/ApproveChoice";
import type { CommentThumbnail } from "../../generated/CommentThumbnail";
import type { ConfirmRow } from "../../generated/ConfirmRow";
import type { DragBackDialog } from "../../generated/DragBackDialog";
import type { GitIdentityForm } from "../../generated/GitIdentityForm";
import type { IconName } from "../../generated/IconName";
import type { OnboardingStepId } from "../../generated/OnboardingStepId";
import type { ReportOpenParams } from "../../generated/ReportOpenParams";
import type { GlobalSection } from "./settings";

/** A confirmation's texts, from the view models, and what its confirm button does. */
export interface Confirmation {
  title: string;
  body: string;
  /** Lines under the body, each with its icon (Park: what is kept). */
  rows?: readonly ConfirmRow[];
  /** `null`: only the cancel button (the unsent-text question for a draft the tool can't clear, Q-N). */
  confirmLabel: string | null;
  cancelLabel?: string;
  confirmIcon?: IconName;
  /** A destructive confirm button, with Cancel as the default (`dialog.rs`). */
  destructive: boolean;
  /** Cancel as the default while the confirm button stays primary (Park all). */
  cancelDefault?: boolean;
  /** A third button before Cancel; it closes the dialog, then runs (the leave question's Discard changes, Q415). */
  extra?: { label: string; onClick: () => unknown };
  onConfirm: () => unknown;
}

/** Each overlay id with what it is opened with (`undefined`: nothing). */
export interface OverlayParams {
  /** The quick switcher, ⌘P (T-0219). */
  quick_switcher: undefined;
  /** The command palette, ⌘K / ⇧⌘P (T-0393); `terminal`: a terminal had the keyboard when it opened. */
  command_palette: { terminal: boolean } | undefined;
  /** The ticket editor's whole form on a new Backlog ticket (T-0228); `title` and `body` start it: what was typed in
   * the box for new work before More options… (`fromBox`: the box keeps it until a ticket is created), or a file's path in
   * the description (New ticket from this file, FTB-03). */
  new_ticket: { title?: string; body?: string; fromBox?: boolean } | undefined;
  /** The ticket editor on a ticket (T-0228). */
  edit_ticket: { ticket: string };
  /** The app's Settings, on a section if given (T-0234); `agent`: Settings → Agents on that agent's page (its id;
   * Open agent…, T-0411). A project's settings are its own page (`openProjectSettings`). */
  settings: { section?: GlobalSection; agent?: string } | undefined;
  /** The running processes (T-0232). */
  processes: undefined;
  /** The run-command dialog of a project's first Run (T-0232); `ask`: Settings' Ask the agent was the click. */
  run_command: { project: string; ticket?: string; ask?: boolean; app?: string | null };
  /** Recovery and Cleanup (T-0235). */
  cleanup: undefined;
  /** The project's Terminals drawer: Project terminals only (T-0242, T-0331). */
  terminals: undefined;
  /** The after-restart summary (T-0235, T-0244). */
  after_restart: undefined;
  /** An agent tool's Sign in… terminal, opened from a tool problem (the sheet, the agent panel, a notice): the same
   * dialog Settings → Agents shows. The drawer lists only the project's own terminals, so it can't show this one. */
  sign_in: AgentToolSignIn;
  /** The Activity popover of the header, ⇧⌘U (T-0329; the agent limits since T-0219). */
  activity: undefined;
  /** Add project (T-0233), with a folder already picked or dropped on the window. */
  add_project: { folder?: string } | undefined;
  /** First run (T-0233), over the whole window; Check tools opens it on its tools step. */
  onboarding: { step?: OnboardingStepId } | undefined;
  /** The delete confirmation of a never-started ticket (T-0221). */
  delete_ticket: { ticket: string };
  /** The Close ticket confirmation (T-0222). */
  close_ticket: { ticket: string };
  /** Move to next stage (was Submit for review) with an optional note (T-0222, T-0406). */
  submit_for_review: { ticket: string };
  /** Start from scratch after a failed resume: what is kept and lost (T-0235). */
  start_from_scratch: { ticket: string };
  /** Send back with a note (T-0226), from every human check (d0093). */
  send_back: { ticket: string };
  /** Change agent… on a stage that waits for its tool: another agent for it whose tool has its skill and can run
   * now; `stage` (its id) for another stage that hasn't started. */
  change_agent: { ticket: string; stage?: string };
  /** A card dragged back to an earlier Agent stage or agent check: its instructions or note (T-0421, FLW-18). */
  drag_back: DragBackDialog;
  /** Send back with PR comments and an optional note (FLW-16, T-0424). */
  send_back_pr: { ticket: string };
  /** Change… on a recorded decision (T-0288): Send back. */
  change_decision: { id: string };
  /** The Local merge confirmation (T-0226); `menu` starts at the Approve ▾ choice (⌘↵); `choice` goes straight to
   * that choice's dialog (the review bar's Approve, named by its action, and the rows of its menu). */
  local_merge: { ticket: string; menu?: boolean; choice?: ApproveChoice };
  /** Merge into another ticket…: the picker of tickets whose branch can take this one's (Approve ▾). */
  merge_into: { ticket: string };
  /** "Done, don't merge" (Approve ▾, a branch with commits): the confirmation naming what won't land. */
  done_not_merged: { ticket: string };
  /** The Open PR / Push to PR dialog (T-0227). `approveAnyway`: the user already chose Approve anyway after "changed
   * since you reviewed it"; `intro`: Push to PR shows what goes to the PR first (the review flow's ⌘↵ / Approve). */
  open_pr: { ticket: string; approveAnyway?: boolean; intro?: boolean };
  /** Any other confirmation (Park, Reopen, Resume all, Restart now, Remove project…), drawn by T-0217's dialog. */
  confirm: Confirmation;
  /** A start when git has no name and email: the two fields; `save` ("Save and start") saves them, then runs `then`
   * (the start again). `onCancel` runs on Cancel and Esc. */
  git_identity: {
    form: GitIdentityForm;
    save: string;
    cancel: string;
    then: () => unknown;
    onCancel?: () => unknown;
  };
  /** Restart now's confirmation, naming who gets interrupted (T-0231). */
  restart_service: undefined;
  /** Resume all's confirmation listing the sessions it resumes (T-0230): the Needs you strip's for its project
   * (T-0459), the banner's for every project. */
  resume_all: { project?: string } | undefined;
  /** The ? shortcut list (T-0394, KEY-07), from anywhere; Esc goes back. */
  shortcuts: undefined;
  /** The developer-only design gallery, over the whole window (T-0217, `OTAQ_GALLERY=1`). */
  gallery: undefined;
  /** A description over the whole window (T-0260); `owner` is the form or Ticket tab that draws into it. */
  full_view: { owner: string };
  /** Apply now on a project's waiting flow change (T-0439, APN-02), from the flow change banner. */
  apply_now: { project: string };
  /** A comment's report, or a paste not posted yet, read-only over the sheet (CMT-12, T-0447). */
  report: ReportOpenParams;
  /** A comment's image full size over the sheet (tuq 004): its ticket, its comment's number and its thumbnail. */
  image: { ticket: string; n: number; thumbnail: CommentThumbnail };
}

export type OverlayId = keyof OverlayParams;

/**
 * Whether the overlay is modal: while one is open the app's shortcuts don't run (`modal.rs`, T-0215's dispatcher
 * asks `modalOpen`).
 */
export const OVERLAY_MODAL: Record<OverlayId, boolean> = {
  quick_switcher: false,
  command_palette: false,
  new_ticket: true,
  edit_ticket: true,
  settings: true,
  processes: false,
  run_command: true,
  cleanup: true,
  terminals: false,
  after_restart: true,
  sign_in: true,
  activity: false,
  add_project: true,
  onboarding: true,
  delete_ticket: true,
  close_ticket: true,
  submit_for_review: true,
  start_from_scratch: true,
  send_back: true,
  change_agent: true,
  drag_back: true,
  send_back_pr: true,
  change_decision: true,
  local_merge: true,
  merge_into: true,
  done_not_merged: true,
  open_pr: true,
  confirm: true,
  git_identity: true,
  restart_service: true,
  resume_all: true,
  shortcuts: true,
  gallery: true,
  full_view: true,
  apply_now: true,
  report: true,
  image: true,
};

export const OVERLAY_IDS = Object.keys(OVERLAY_MODAL) as OverlayId[];

/** What an overlay's component gets: what it was opened with, and how to close itself. */
export interface OverlayProps<K extends OverlayId> {
  params: OverlayParams[K];
  close: () => void;
}

export type OverlayEntries = { [K in OverlayId]: { Component: ComponentType<OverlayProps<K>> } };
