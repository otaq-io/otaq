// Decisions in the Ticket tab (DEC-03, T-0288; *05 → Row · Recorded decisions (0084)*): the agent's recorded
// decisions under "What and why", read-only while it works (no Change…: that is the review's), and with what the
// review did after approval. What it says is Rust's (`review.ticket_decisions`, `decision_item.rs`); nothing when the
// ticket has none.
import { useCallback } from "react";

import type { TicketProps } from "../../app/registry";
import { DecisionsList } from "../../components";
import type { DecisionsSection } from "../../generated/DecisionsSection";
import { useServiceView } from "../../service";
import { request } from "../../tauri";

/** A decision recorded, kept or changed changes the ticket. */
export const DECISION_EVENTS = ["ticket_changed", "card_status_changed"] as const;

export function TicketDecisions({ ticket }: TicketProps) {
  const load = useCallback(
    () => request("review.ticket_decisions", { key: ticket }) as Promise<DecisionsSection | null>,
    [ticket],
  );
  const { view } = useServiceView(load, DECISION_EVENTS);
  if (!view) return null;
  return (
    <div className="ticket-decisions">
      <DecisionsList section={view} />
    </div>
  );
}
