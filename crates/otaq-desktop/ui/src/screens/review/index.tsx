// Area 07 · Review (T-0226): the `local_merge` overlay (Approve ▾, changed since review, the Local merge
// confirmation and its results) and the `send_back` overlay, and ⌘↵ / ⌘⌫ on the open ticket. The sheet's Approve and
// Send back buttons and the board's approve choice open the same overlays. `change_decision` is Change… on a
// recorded decision (T-0288). What they say is Rust's
// (`src/api/review.rs`, `otaq-view`'s `review_page.rs`).
import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import { selection } from "../../app/selection";
import type { ReviewAvailable } from "../../generated/ReviewAvailable";
import { request } from "../../tauri";
import { passTicket, SendBackPrDialog } from "./CheckDialogs";
import { ChangeDecisionDialog, LocalMergeDialog, SendBackDialog } from "./Dialogs";
import "./review.css";

/** Opens a review dialog for the open ticket when it takes one now; the key is the app's either way. */
function onOpenTicket(open: (ticket: string, available: ReviewAvailable) => void) {
  return () => {
    const ticket = selection.get().ticket;
    if (ticket === null) return false;
    (request("review.available", { key: ticket }) as Promise<ReviewAvailable>).then(
      (available) => {
        open(ticket, available);
      },
      () => undefined,
    );
    return true;
  };
}

export const review: Screen = (app) => {
  app.overlays.register("local_merge", { Component: LocalMergeDialog });
  app.overlays.register("send_back", { Component: SendBackDialog });
  app.overlays.register("change_decision", { Component: ChangeDecisionDialog });
  app.overlays.register("send_back_pr", { Component: SendBackPrDialog });
  app.shortcuts.on(
    "approve",
    onOpenTicket((ticket, available) => {
      // In another flow a human check's Approve is its Pass, which moves it on (FLW-14, d0093).
      if (available.pass) void passTicket(ticket, true);
      else if (available.approve) openOverlay("local_merge", { ticket, menu: true });
    }),
  );
  app.shortcuts.on(
    "send_back",
    onOpenTicket((ticket, available) => {
      if (available.send_back) openOverlay("send_back", { ticket });
    }),
  );
  // Open PR (0097, palette only unless the user gives it a key): Approve ▾'s Open PR, which asks first when the
  // ticket changed since the review (REV-06), as the card's button does.
  app.shortcuts.on(
    "open_pr",
    onOpenTicket((ticket, available) => {
      if (available.approve) openOverlay("open_pr", { ticket });
    }),
  );
};
