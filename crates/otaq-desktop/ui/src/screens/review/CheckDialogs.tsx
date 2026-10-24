// The dialogs of a human check and an open PR (FLW-14, FLW-16, CMT-13; d0108, d0109; `s03-50`): **Fail** ("Fail Review
// on SHOP-18?", what's wrong as a comment for Dev, Fail and send to Dev) and **Send back with PR comments** (an
// optional comment, Send back), and what Pass opens. Both write in the comment box (a long paste becomes its report,
// the limit applies). Their words come from `review.fail_view` and `review.send_back_pr_view` (`check_dialogs.rs`);
// Fail sends `review.send_back` (`ticket.fail`), Send back `review.send_back_pr`, Pass `review.pass`.
import { useCallback, useEffect, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayParams } from "../../app/registry/overlays";
import type { OverlayProps } from "../../app/registry";
import { Button, Dialog } from "../../components";
import type { SheetSent } from "../../generated/SheetSent";
import type { ReviewPassed } from "../../generated/ReviewPassed";
import type { SendBackParams } from "../../generated/SendBackParams";
import type { SendBackPrParams } from "../../generated/SendBackPrParams";
import type { TicketFailView } from "../../generated/TicketFailView";
import type { TicketSendBackPrView } from "../../generated/TicketSendBackPrView";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { CommentBox } from "../editor/CommentBox";
import { type BoxValue, boxState, EMPTY_BOX } from "../editor/commentRules";
import { afterSend } from "./Dialogs";

/** What a Pass opens next: nothing once the card moved on, else today's delivery choices (`menu`: the choice between
 * Local merge and Open PR, also while recorded decisions are open, DEC-05). */
export function passOverlay(
  ticket: string,
  passed: ReviewPassed,
  menu: boolean,
):
  | { id: "local_merge"; params: OverlayParams["local_merge"] }
  | { id: "open_pr"; params: OverlayParams["open_pr"] }
  | null {
  switch (passed) {
    case "moved":
      return null;
    case "local_merge":
      return { id: "local_merge", params: menu ? { ticket, menu: true } : { ticket } };
    case "open_pr":
      return { id: "open_pr", params: { ticket } };
    case "either":
      return { id: "local_merge", params: { ticket, menu: true } };
  }
}

/** Pass on a human check (the card's and the panel's button): a refusal is Rust's toast. */
export function passTicket(ticket: string, menu: boolean): Promise<void> {
  return (request("review.pass", { key: ticket }) as Promise<ReviewPassed>).then(
    (passed) => {
      const next = passOverlay(ticket, passed, menu);
      if (next?.id === "local_merge") openOverlay("local_merge", next.params);
      if (next?.id === "open_pr") openOverlay("open_pr", next.params);
    },
    () => undefined,
  );
}

/** Sends with the dialog's busy state and the service's refusal under the box; `ready` is the box's own rule (over
 * the limit, or only a pasted report: not yet). */
function useSend(send: (clear: boolean) => Promise<unknown>, close: () => void, ready: boolean) {
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const confirm = () => {
    if (busy || !ready) return;
    setBusy(true);
    setRefusal(null);
    send(false).then(close, (e: unknown) => {
      setRefusal(asRequestError(e).message);
      setBusy(false);
    });
  };
  return { busy, refusal, confirm };
}

/** Fail on a human check (`s03-50` → *Fail asks for comments*): the comments go to the nearest earlier Agent stage. */
export function FailDialog({ params, close }: OverlayProps<"fail">) {
  const ticket = params.ticket;
  const load = useCallback(
    () => request("review.fail_view", { key: ticket }) as Promise<TicketFailView>,
    [ticket],
  );
  const { view, error } = useServiceView(load, []);
  const [comment, setComment] = useState<BoxValue>(EMPTY_BOX);
  const ready = view ? boxState(comment, view.comment.limits, view.comment.optional).canSubmit : false;
  const send = (clear: boolean): Promise<unknown> =>
    (
      request("review.send_back", {
        key: ticket,
        note: comment.text,
        report: comment.pasted?.text,
        clear,
        comments: 0,
      } satisfies SendBackParams) as Promise<SheetSent>
    ).then((sent) => {
      afterSend(ticket, sent, () => send(true));
    });
  const { busy, refusal, confirm } = useSend(send, close, ready);
  useEffect(() => {
    if (error && !view) close();
  }, [error, view, close]);
  if (!view) return null;
  return (
    <Dialog
      title={view.title}
      width={480}
      hint={view.hint}
      onConfirm={confirm}
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label={view.cancel} onClick={close} />
          <Button icon="undo-2" label={view.confirm} loading={busy} disabled={!ready} onClick={confirm} />
        </>
      }
    >
      <div className="review-body">
        <CommentBox
          ticket={ticket}
          view={view.comment}
          value={comment}
          onChange={setComment}
          onSubmit={confirm}
        />
        {view.footnote !== null && <p className="review-muted">{view.footnote}</p>}
        {refusal !== null && <p className="review-error">{refusal}</p>}
      </div>
    </Dialog>
  );
}

/** Send back with PR comments (`s03-50` → *Sending an open PR back*): the note is optional. */
export function SendBackPrDialog({ params, close }: OverlayProps<"send_back_pr">) {
  const ticket = params.ticket;
  const load = useCallback(
    () => request("review.send_back_pr_view", { key: ticket }) as Promise<TicketSendBackPrView>,
    [ticket],
  );
  const { view, error } = useServiceView(load, []);
  const [comment, setComment] = useState<BoxValue>(EMPTY_BOX);
  const ready = view ? boxState(comment, view.comment.limits, view.comment.optional).canSubmit : false;
  const send = (clear: boolean): Promise<unknown> =>
    (
      request("review.send_back_pr", {
        key: ticket,
        note: comment.text,
        report: comment.pasted?.text,
        clear,
      } satisfies SendBackPrParams) as Promise<SheetSent>
    ).then((sent) => {
      afterSend(ticket, sent, () => send(true));
    });
  const { busy, refusal, confirm } = useSend(send, close, ready);
  useEffect(() => {
    if (error && !view) close();
  }, [error, view, close]);
  if (!view) return null;
  return (
    <Dialog
      title={view.title}
      width={480}
      hint={view.hint}
      onConfirm={confirm}
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label={view.cancel} onClick={close} />
          <Button icon="undo-2" label={view.confirm} loading={busy} disabled={!ready} onClick={confirm} />
        </>
      }
    >
      <div className="review-body">
        <p className="review-text">{view.text}</p>
        <CommentBox
          ticket={ticket}
          view={view.comment}
          value={comment}
          onChange={setComment}
          onSubmit={confirm}
        />
        {refusal !== null && <p className="review-error">{refusal}</p>}
      </div>
    </Dialog>
  );
}
