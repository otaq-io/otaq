// The Apply now window's moments of interaction (T-0439, APN-02): the picks kept until Apply now, the line a refresh
// wrote, and the two requests. What it says is Rust's (`src/api/apply_now.rs`, `otaq-view`'s `apply_now.rs`).
import type { ApplyNowAnswer } from "../../generated/ApplyNowAnswer";
import type { ApplyNowChanged } from "../../generated/ApplyNowChanged";
import type { ApplyNowLoad } from "../../generated/ApplyNowLoad";
import type { ApplyNowView } from "../../generated/ApplyNowView";
import { request } from "../../tauri";

export type Ask = typeof request;

/** A ticket's key → the id of its picked entry (`backlog`, `ready_for_dev` for Start again, `stage:<id>`). */
export type Picks = Readonly<Record<string, string>>;

/** What the window refetches on: the waiting change, and the tickets and sessions its plan stands on. */
export const REFETCH_ON = ["flow_changed", "ticket_changed", "card_status_changed"];

/** `picks` with `ticket` on `id`. */
export function withPick(picks: Picks, ticket: string, id: string): Picks {
  return { ...picks, [ticket]: id };
}

/** The window with the picks so far, and the line the last refresh wrote. */
export function loadApplyNow(
  project: string,
  picks: Picks,
  changed: ApplyNowChanged | null,
  ask: Ask = request,
): Promise<ApplyNowLoad> {
  return ask(
    "apply_now.view",
    changed === null ? { project, picks } : { project, picks, changed },
  ) as Promise<ApplyNowLoad>;
}

/** Apply now with the picks: closes the window, or gives the line for the refreshed one. */
export async function applyNow(
  view: ApplyNowView,
  picks: Picks,
  ask: Ask = request,
): Promise<{ close: true } | { close: false; changed: ApplyNowChanged | null }> {
  const answer = (await ask("apply_now.apply", {
    project: view.project,
    stamp: view.stamp,
    picks,
    basis: view.basis,
  })) as ApplyNowAnswer;
  if (answer.outcome === "closed") return { close: true };
  const fresh = answer.view;
  return {
    close: false,
    changed: fresh.changed === null ? null : { stamp: fresh.stamp, line: fresh.changed },
  };
}
