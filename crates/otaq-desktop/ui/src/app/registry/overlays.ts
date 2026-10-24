// Every overlay of the page (dialogs, popovers, drawers), fixed here so screens open one another's by id
// (`openOverlay("activity")`) without editing a shared list. Each overlay's own screen registers its component; opening
// one that isn't registered yet does nothing visible. Menus and tooltips tied to one element stay local to their
// component (they may portal into the overlay host, `OVERLAY_HOST_ID`).
import type { ComponentType } from "react";

import type { BlockedDialog } from "../../generated/BlockedDialog";
import type { ConfirmRow } from "../../generated/ConfirmRow";
import type { DragBackDialog } from "../../generated/DragBackDialog";
import type { FlowEditorFrom } from "../../generated/FlowEditorFrom";
import type { IconName } from "../../generated/IconName";
import type { OnboardingStepId } from "../../generated/OnboardingStepId";
import type { ReportOpenParams } from "../../generated/ReportOpenParams";
import type { UsageDataDialog } from "../../generated/UsageDataDialog";
import type { GlobalSection, ProjectSection } from "./settings";

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
/** A lead's suggestion behind a close or delete confirmation (LEAD-08): its line, and the id its confirm sends. */
export interface SuggestionConfirm {
  id: string;
  /** "Suggested by Wishlist lead: “PAY-123 doesn't ask for an export.”" */
  line: string;
}

export interface OverlayParams {
  /** The quick switcher, ⌘P (T-0219). */
  quick_switcher: undefined;
  /** The command palette, ⌘K / ⇧⌘P (T-0393); `terminal`: a terminal had the keyboard when it opened. */
  command_palette: { terminal: boolean } | undefined;
  /** The ticket editor on a new Backlog ticket, in an epic if given (T-0228); `body` starts the description, and the
   * whole form opens (New ticket from this file, FTB-03). */
  new_ticket: { epic?: string; body?: string } | undefined;
  /** The ticket editor on a ticket (T-0228). */
  edit_ticket: { ticket: string };
  /** Settings, on a project's page and on a section if given (T-0234); `agent`: Settings → Agents on that agent's
   * page (its id; Open agent…, T-0411). */
  settings: { project?: string; section?: GlobalSection | ProjectSection; agent?: string } | undefined;
  /** The running processes (T-0232). */
  processes: undefined;
  /** The run-command dialog of a project's first Run (T-0232); `ask`: Settings' Ask the agent was the click. */
  run_command: { project: string; ticket?: string; ask?: boolean };
  /** Recovery and Cleanup (T-0235). */
  cleanup: undefined;
  /** The project's Terminals drawer: Project terminals only (T-0242, T-0331). */
  terminals: undefined;
  /** The after-restart summary (T-0235, T-0244). */
  after_restart: undefined;
  /** The Activity popover of the header, ⇧⌘U (T-0329; the agent limits since T-0219). */
  activity: undefined;
  /** Add project (T-0233), with a folder already picked or dropped on the window. */
  add_project: { folder?: string } | undefined;
  /** First run (T-0233), over the whole window; Check tools opens it on its tools step, and a start while first run's
   * project is still in Flow setup on its Flow step (`project`, ASK-04). */
  onboarding: { step?: OnboardingStepId; project?: string } | undefined;
  /** "Help improve Otaq": anonymous usage data, asked once after first run (d0085). */
  usage_data: UsageDataDialog;
  /** The delete confirmation of a never-started ticket (T-0221); from a lead's suggestion, with its line (T-0292). */
  delete_ticket: { ticket: string; suggestion?: SuggestionConfirm };
  /** The Close ticket confirmation (T-0222); from a lead's suggestion, with its line (T-0292). */
  close_ticket: { ticket: string; suggestion?: SuggestionConfirm };
  /** Move to next stage (was Submit for review) with an optional note (T-0222, T-0406). */
  submit_for_review: { ticket: string };
  /** Start from scratch after a failed resume: what is kept and lost (T-0235). */
  start_from_scratch: { ticket: string };
  /** Move to epic… on a never-started ticket, from the card menu (T-0223's epic dialogs). */
  move_to_epic: { ticket: string };
  /** Close epic…'s confirmation, from the card menu (T-0223's epic dialogs). */
  close_epic: { epic: string };
  /** Send back with a note (T-0226). */
  send_back: { ticket: string };
  /** A card dragged back to an earlier Agent stage or agent check: its instructions or note (T-0421, FLW-18). */
  drag_back: DragBackDialog;
  /** A blocked ticket dragged past Ready for Dev or started (DPV-03, T-0463): why it waits; Keep it waiting only. */
  blocked: BlockedDialog;
  /** Fail on a human check with what's wrong (FLW-14, T-0424). */
  fail: { ticket: string };
  /** Send back with PR comments and an optional note (FLW-16, T-0424). */
  send_back_pr: { ticket: string };
  /** Change… on a recorded decision (T-0288): Send back on a ticket, Add a fix child in an epic's review. */
  change_decision: { id: string };
  /** The Local merge confirmation (T-0226); `menu` starts at the Approve ▾ choice (⌘↵). */
  local_merge: { ticket: string; menu?: boolean };
  /** The Open PR / Push to PR dialog (T-0227). `approveAnyway`: the user already chose Approve anyway after "changed
   * since you reviewed it"; `intro`: Push to PR shows what goes to the PR first (the review flow's ⌘↵ / Approve). */
  open_pr: { ticket: string; approveAnyway?: boolean; intro?: boolean };
  /** Any other confirmation (Park, Reopen, Resume all, Restart now, Remove project…), drawn by T-0217's dialog. */
  confirm: Confirmation;
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
  /** The flow editor over the window (T-0471, FED-01, FED-09): the project's flow, from the project menu, a column's
   * ⋯ or Settings → Flow; a stage to select (Edit <stage>…) or to add a new one after (Add a stage after <stage>;
   * `null`: first, after Ready for Dev). Onboarding's Flow step and the Flow setup page embed its body. */
  flow_editor: { project: string; from: FlowEditorFrom; select_stage?: string; add_after?: string | null };
  /** A comment's report, or a paste not posted yet, read-only over the sheet (CMT-12, T-0447). */
  report: ReportOpenParams;
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
  activity: false,
  add_project: true,
  onboarding: true,
  usage_data: true,
  delete_ticket: true,
  close_ticket: true,
  submit_for_review: true,
  start_from_scratch: true,
  move_to_epic: true,
  close_epic: true,
  send_back: true,
  drag_back: true,
  blocked: true,
  fail: true,
  send_back_pr: true,
  change_decision: true,
  local_merge: true,
  open_pr: true,
  confirm: true,
  restart_service: true,
  resume_all: true,
  shortcuts: true,
  gallery: true,
  full_view: true,
  apply_now: true,
  flow_editor: true,
  report: true,
};

export const OVERLAY_IDS = Object.keys(OVERLAY_MODAL) as OverlayId[];

/** What an overlay's component gets: what it was opened with, and how to close itself. */
export interface OverlayProps<K extends OverlayId> {
  params: OverlayParams[K];
  close: () => void;
}

export type OverlayEntries = { [K in OverlayId]: { Component: ComponentType<OverlayProps<K>> } };
