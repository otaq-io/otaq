// The page's side of the banner area and the toasts (T-0231). Which banners and toasts show, their wording and their
// buttons come from Rust (`notices.view`, then every change as a `notices` event); this file only routes a click to
// the request or the dialog it stands for, and opens what a notification click names.
import { type OverlayStack, overlays } from "../../app/overlays";
import {
  openTicket,
  requestTerminalFocus,
  type Selection,
  selection,
  selectProject,
  type TerminalFocusRequest,
  terminalFocusRequests,
} from "../../app/selection";
import { createStore, type Store } from "../../app/store";
import type { Banner } from "../../generated/Banner";
import type { BannerButtonId } from "../../generated/BannerButtonId";
import type { BannerKind } from "../../generated/BannerKind";
import type { LimitLine } from "../../generated/LimitLine";
import type { Notices } from "../../generated/Notices";
import type { NotificationTarget } from "../../generated/NotificationTarget";
import { asRequestError, type PageEvents, pageEvents } from "../../service";
import { request, windowAction } from "../../tauri";
import { AGENT_PANEL_ROUTES, type AgentPanelRoutes, showAgentInPanel } from "../agent_panel/logic";
import { next } from "../strip/logic";

/** The banner rows and toasts as last heard; `undefined` until the first answer. */
export const noticesStore: Store<Notices | undefined> = createStore<Notices | undefined>(undefined);

/** Whether only Restart works ("Restart required"): screens disable everything else that talks to the service. */
export function onlyRestartAllowed(notices: Notices | undefined): boolean {
  return notices?.only_restart_allowed ?? false;
}

/** Follows the notices: the view now (`notices.view`), then each change. Returns the stop. */
export function watchNotices(
  onChange: (notices: Notices) => void,
  events: PageEvents = pageEvents,
  ask: typeof request = request,
): () => void {
  let stopped = false;
  let changed = false;
  const listening = events.listen((event) => {
    if (event.kind !== "notices" || stopped) return;
    changed = true;
    onChange(event.notices);
  });
  listening.ready
    .then(() => ask("notices.view"))
    .then(
      (notices) => {
        // A change that arrived meanwhile is newer than this answer.
        if (!stopped && !changed) onChange(notices as Notices);
      },
      // The Rust side computes the banners itself; a failed first load only waits for the next change.
      () => undefined,
    );
  return () => {
    stopped = true;
    listening.stop();
  };
}

/** What a banner button does, besides the requests: the dialogs. */
export interface BannerRoutes {
  ask: typeof request;
  overlays: Pick<OverlayStack, "open">;
  /** The project on screen (the flow change banner's); the selection's by default. */
  project?: () => string | null;
}

const ROUTES: BannerRoutes = { ask: request, overlays };

/** A banner button was pressed. Restarts and resumes only ever start here, on the user's click (d0020, d0022). */
export async function pressBanner(
  kind: BannerKind,
  button: BannerButtonId,
  { ask, overlays: stack, project = () => selection.get().project }: BannerRoutes = ROUTES,
): Promise<void> {
  // The notifications ask (ATT-13): Allow brings up macOS's question in the app; Not now only answers the ask.
  if (kind === "notifications_ask") {
    await ask(button === "secondary" ? "notices.notifications_not_now" : "notices.notifications_allow");
    return;
  }
  // A flow change waiting (FLW-06, APN-01): Apply now opens its window on the project on screen; Cancel change asks
  // no question (starts resume).
  if (kind === "flow_waiting") {
    const on = project();
    if (button === "secondary") await ask("notices.flow_cancel", { project: null });
    else if (on !== null) stack.open("apply_now", { project: on });
    return;
  }
  // "Review & resume": the after-restart summary sheet, where each ticket resumes on its own (T-0235).
  if (button === "secondary") {
    stack.open("after_restart");
    return;
  }
  if (button === "resume") {
    await ask("notices.resume_after_limit");
    return;
  }
  switch (kind) {
    case "service_update_ready":
    case "restart_required":
      stack.open("restart_service");
      return;
    // Nothing runs any more, so starting interrupts no one: no confirmation.
    case "service_stopped":
      await ask("notices.restart");
      return;
    // The confirmation listing every project's sessions (T-0230); it closes itself when nothing is left to resume.
    case "sessions_interrupted":
      stack.open("resume_all");
      return;
    case "permission_warning":
      await ask("notices.open_docs");
      return;
    case "usage_limit":
      await ask("notices.resume_after_limit");
      return;
    // macOS blocks the notifications (ATT-14): Otaq's entry in System Settings.
    case "notifications_blocked":
      await ask("notices.open_notification_settings");
      return;
    // The folder trust note and warning have no button (PRJ-08); nor has the paused banner (Resume is the title
    // bar's, PAU-08).
    case "paused":
    case "trust_failed":
    case "trust_note":
    case "reconnecting":
      return;
  }
}

/**
 * The usage limit's lines to draw, one per tool (USG-10): with several tools connected, or when a line has a button
 * besides Resume (Sign in…). Otherwise none, and the banner stays the one-tool banner of M22 with its one Resume
 * (d0086).
 */
export function limitLinesOf(banner: Banner): readonly LimitLine[] {
  const several = banner.lines.some((line) => line.avatar !== null);
  const more = banner.lines.some((line) => line.buttons.some((b) => b.action.kind !== "resume_tool"));
  return several || more ? banner.lines : [];
}

/** Where opening a notification's target lands: the selection, the terminal focus requests, the Project agent panel
 * (its `selection` is the same store) and the Needs you strip's Next. */
export interface OpenRoutes {
  selection: Store<Selection>;
  focus: Store<TerminalFocusRequest | null>;
  agentPanel: AgentPanelRoutes;
  next: () => void;
}

const OPEN_ROUTES: OpenRoutes = {
  selection,
  focus: terminalFocusRequests,
  agentPanel: AGENT_PANEL_ROUTES,
  next,
};

/**
 * Opens what a notification names (d0090 "What a click opens"), its project on screen: a ticket on Session with the
 * terminal focused (the check's side for a validator's prompt), a ticket on Files or as it opens (the status icon's
 * lines), a Project agent in the agent panel, the item the
 * Needs you strip's Next opens (a grouped notification or a summary, NYS-03), or the board. An item answered meanwhile opens the same place.
 */
export function openTarget(target: NotificationTarget, routes: OpenRoutes = OPEN_ROUTES): void {
  switch (target.kind) {
    case "ticket":
    case "validator":
      selectProject(target.project, routes.selection);
      // Session, on the check's side for a validator's notification.
      openTicket(target.ticket, target.kind === "validator" ? "check" : "agent", routes.selection);
      requestTerminalFocus(target.ticket, routes.focus);
      break;
    // The status icon's lines (d0139, TRY-06): a check opens on Files to review it; Interrupted and Start failed open
    // the ticket as it opens, nothing resumed.
    case "review":
      selectProject(target.project, routes.selection);
      openTicket(target.ticket, "files", routes.selection);
      break;
    case "sheet":
      selectProject(target.project, routes.selection);
      openTicket(target.ticket, undefined, routes.selection);
      break;
    case "project_agent":
      showAgentInPanel(target.project, target.agent, routes.agentPanel);
      break;
    case "needs_you":
      routes.next();
      break;
    case "board":
      if (target.project !== null) selectProject(target.project, routes.selection);
      break;
  }
}

/**
 * Notification clicks (ATT-11): the target the app was started with (`OTAQ_OPEN`), once, then every
 * `open_from_notification` event, which also brings the window forward. Returns the stop.
 */
export function followNotifications(
  events: PageEvents = pageEvents,
  ask: typeof request = request,
  open: (target: NotificationTarget) => void = openTarget,
  bringForward: () => Promise<void> = () => windowAction("focus"),
): () => void {
  let stopped = false;
  const listening = events.listen((event) => {
    if (stopped || event.kind !== "service" || event.name !== "open_from_notification") return;
    void bringForward().catch(() => undefined);
    ask("notices.open_target", { target: event.event.target }).then(
      (target) => {
        if (!stopped) open(target as NotificationTarget);
      },
      (error: unknown) => {
        console.warn("can't open the notification's target:", asRequestError(error).message);
      },
    );
  });
  listening.ready
    .then(() => ask("notices.open_at_start"))
    .then(
      (target) => {
        if (!stopped && target !== null) open(target as NotificationTarget);
      },
      () => undefined,
    );
  return () => {
    stopped = true;
    listening.stop();
  };
}
