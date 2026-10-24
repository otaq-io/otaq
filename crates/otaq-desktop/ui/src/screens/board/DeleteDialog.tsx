// The delete confirmation of a never-started ticket (*05 · Ticket side panel → Delete ticket · confirm*, *Epic dialogs
// → Delete epic · confirm*; TKT-08, d0060, d0018): the `delete_ticket` overlay, one dialog for every place that
// deletes (the card menu, the sheet's "…" menu, the Project agent's summary): `openOverlay("delete_ticket", { ticket
// })`. Its texts are the service's preview worded by `delete.rs` (`board.delete_dialog`): the ticket, that it never
// started, the links that go and an epic's children. Cancel is the default; nothing is deleted without the confirm
// click (`board.delete`, which toasts what went). A refusal (the ticket started) shows in the dialog. Opened from a
// lead's suggestion (LEAD-08), it adds "Suggested by Wishlist lead: “…”" and its confirm goes through the suggestion
// (`suggestion.confirm`).
import { useEffect, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { closeTicket, selection } from "../../app/selection";
import { ConfirmDialog } from "../../components";
import type { BoardTicketParams } from "../../generated/BoardTicketParams";
import type { DeleteDialogView } from "../../generated/DeleteDialogView";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import { confirmSuggestion } from "../strip/logic";
import { SuggestedBy } from "../strip/SuggestedBy";

export interface DeleteBodyProps {
  view: DeleteDialogView;
  /** A refusal of the confirm click, after the dialog opened. */
  error: string | null;
  /** A lead's suggestion: "Suggested by Wishlist lead: “…”". */
  suggested?: string;
}

/** The paragraph, and the service's refusal under it. */
export function DeleteBody({ view, error, suggested }: DeleteBodyProps) {
  const refusal = error ?? view.error;
  return (
    <>
      {suggested !== undefined && <SuggestedBy line={suggested} />}
      {view.body.length > 0 && <p className="dialog-text">{view.body.join(" ")}</p>}
      {refusal !== null && (
        <p className="board-delete-error" role="alert">
          {refusal}
        </p>
      )}
    </>
  );
}

/** Deletes `ticket`; the open sheet closes if it showed it. */
export function deleteTicket(ticket: string, ask: typeof request = request): Promise<void> {
  const params: BoardTicketParams = { key: ticket };
  return ask("board.delete", params).then(() => {
    if (selection.get().ticket === ticket) closeTicket();
  });
}

export function DeleteDialog({ params, close }: OverlayProps<"delete_ticket">) {
  const { ticket, suggestion } = params;
  const [view, setView] = useState<DeleteDialogView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  useEffect(() => {
    let live = true;
    const ask: BoardTicketParams = { key: ticket };
    (request("board.delete_dialog", ask) as Promise<DeleteDialogView>).then(
      (v) => {
        if (live) setView(v);
      },
      (e: unknown) => {
        if (live) setError(asRequestError(e).message);
      },
    );
    return () => {
      live = false;
    };
  }, [ticket]);
  if (view === null && error === null) return null;
  return (
    <ConfirmDialog
      title={view?.title ?? ticket}
      body={
        view ? (
          <DeleteBody view={view} error={error} suggested={suggestion?.line} />
        ) : (
          <p className="board-delete-error">{error}</p>
        )
      }
      confirmLabel={view?.confirm ?? ""}
      confirmIcon={view?.confirm_icon}
      destructive
      confirmDisabled={view === null || view.error !== null || deleting}
      onConfirm={() => {
        setDeleting(true);
        const done = suggestion
          ? confirmSuggestion(suggestion).then(() => {
              if (selection.get().ticket === ticket) closeTicket();
            })
          : deleteTicket(ticket);
        done.then(close, (e: unknown) => {
          setDeleting(false);
          setError(asRequestError(e).message);
        });
      }}
      onCancel={close}
    />
  );
}
