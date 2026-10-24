// The Lead Status (*01 · Components · Epics on the board → Row · Lead Status (0083)*; LEAD-11, d0083): the bot icon,
// the lead's name and its state. The full form ("Wishlist lead · re-planning") sits on the Backlog epic card; the
// short form (icon + state) on the lead's own line in the track label. Working states are violet, idle is quiet,
// "asks you" turquoise (a pill of its own, except inside a label that is already turquoise), ended is quiet with
// Resume where Rust offers it; an epic with no lead shows only the labelled "Open lead" button ("Split with the
// Project agent" while it has no children either, LEAD-13: it sends the Project agent panel Rust's first message and
// shows the agent panel, d0091). Hovering shows the
// tooltip Rust wrote (*03 → Interactions (0083)*); a click, ↵ or a double click on the status opens the epic sheet
// on its Lead tab. The status is never a drag handle: a press on it doesn't reach the card or the track.
import type { PointerEvent, SyntheticEvent } from "react";

import type { LeadActionKind } from "../generated/LeadActionKind";
import type { LeadSplit } from "../generated/LeadSplit";
import type { LeadStatusView } from "../generated/LeadStatusView";
import { openTicket } from "../app/selection";
import { splitWithProjectAgent } from "../screens/agent_panel/logic";
import { asRequestError } from "../service";
import { request } from "../tauri";
import { Icon } from "../theme/Icon";
import { Tooltip } from "./Tooltip";
import "./LeadStatus.css";

/** Opens the epic sheet on its Lead tab (a click on a Lead Status, ↵ or a double click on the lead line; T-0290). */
export function openLeadTab(epic: string): void {
  openTicket(epic, "lead");
}

/**
 * The status's button: Open lead (`board.lead_open`, then the epic sheet) or Resume (`board.lead_resume`). A refusal
 * ("Usage limit reached · the lead opens once it resets") is a toast in the service's words.
 */
export function leadAction(kind: LeadActionKind, epic: string, ask: typeof request = request): Promise<void> {
  const method = kind === "open" ? "board.lead_open" : "board.lead_resume";
  return ask(method, { key: epic }).then(
    () => {
      if (kind === "open") openLeadTab(epic);
    },
    (error: unknown) => {
      const title = asRequestError(error).message;
      ask("notices.toast", { level: "error", title, text: null, link: null, ticket: null }).catch(
        () => undefined,
      );
    },
  );
}

export interface LeadStatusProps {
  lead: LeadStatusView;
  /** Full on the card; short (no name) on the track's lead line. */
  form: "full" | "short";
  /** Inside a label that is already turquoise: "asks you" draws no pill of its own. */
  plain?: boolean;
  /** A click (or ↵, or a double click) on a live lead's status. */
  onOpen?: (epic: string) => void;
  /** Its button: Open lead or Resume. */
  onAction?: (kind: LeadActionKind, epic: string) => void;
  /** Its button on an epic with no children: Split with the Project agent. */
  onSplit?: (split: LeadSplit) => void;
}

/** Keeps a press or click on the status from reaching the card or the track (it is no drag handle). */
const keep = (e: SyntheticEvent | PointerEvent) => {
  e.stopPropagation();
};

export function LeadStatus({
  lead,
  form,
  plain = false,
  onOpen = openLeadTab,
  onAction = (kind, epic) => {
    void leadAction(kind, epic);
  },
  onSplit = (split) => {
    void splitWithProjectAgent(split, request);
  },
}: LeadStatusProps) {
  const action = lead.action;
  const button = action && (
    <button
      type="button"
      className="lead-status-action"
      onPointerDown={keep}
      onDoubleClick={keep}
      onClick={(e) => {
        keep(e);
        if (action.split) onSplit(action.split);
        else onAction(action.kind, lead.epic);
      }}
    >
      <Icon name={action.icon} size={10} />
      <span>{action.label}</span>
    </button>
  );
  if (lead.name === null) {
    return (
      <Tooltip text={lead.tooltip.title} detail={lead.tooltip.lines}>
        <span className="lead-status" data-tone={lead.tone} data-form={form}>
          {button}
        </span>
      </Tooltip>
    );
  }
  const state = form === "full" ? `· ${lead.state}` : lead.short;
  const open = (e: SyntheticEvent) => {
    keep(e);
    if (lead.opens_tab) onOpen(lead.epic);
  };
  return (
    <span
      className="lead-status"
      data-tone={lead.tone}
      data-form={form}
      data-plain={plain || undefined}
      aria-label={lead.full}
    >
      <Tooltip text={lead.tooltip.title} detail={lead.tooltip.lines}>
        <button
          type="button"
          className="lead-status-main"
          onPointerDown={keep}
          onClick={open}
          onDoubleClick={open}
          onKeyDown={(e) => {
            // ↵ opens it as a click does, without reaching the card's or the track's own keys.
            if (e.key === "Enter") keep(e);
          }}
        >
          <span className="lead-status-icon">
            <Icon name={lead.icon} size={12} />
          </span>
          {form === "full" && <span className="lead-status-name">{lead.name}</span>}
          <span className="lead-status-state">{state}</span>
        </button>
      </Tooltip>
      {button}
    </span>
  );
}
