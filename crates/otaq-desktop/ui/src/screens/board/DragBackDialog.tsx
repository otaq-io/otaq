// The drag-back dialog (*03 → Dragging a card back to an earlier stage*, s03-50/s03-51; FLW-18, CMT-13; d0108, d0109):
// a card dropped on an earlier Agent stage asks for a comment for that stage ("Send SHOP-19 back to Dev?" · "Comment
// for Dev (Rust developer)" · Send to Dev), one dropped on an earlier agent check for a comment for the check ("Run
// Security again on SHOP-18?" · Run Security). The comment is posted for every later stage, in the comment box (a long
// paste becomes its report, the limit applies). When an agent works where the card is, the dialog says it stops and
// the button reads "Stop and send to Dev". Its words are `board_page::drag_back_dialog`'s, opened from `board.drop`'s
// answer; the confirm sends `board.drag_back`. Cancel and Esc leave the card where it was; a refusal stays in the
// dialog.
import { useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps } from "../../app/registry";
import { Button, Dialog } from "../../components";
import type { BoardDragBackParams } from "../../generated/BoardDragBackParams";
import type { DragBackDialog as DragBackView } from "../../generated/DragBackDialog";
import type { SheetSent } from "../../generated/SheetSent";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { CommentBox } from "../editor/CommentBox";
import { type BoxValue, boxState, EMPTY_BOX } from "../editor/commentRules";

/** Sends the drag back with `comment` (its text and pasted report); unsent text in the agent's terminal asks first,
 * then sends the same again with `clear` (d0048). */
export function sendDragBack(
  view: DragBackView,
  comment: BoxValue,
  clear = false,
  ask: typeof request = request,
): Promise<void> {
  const params: BoardDragBackParams = { key: view.key, target: view.target, note: comment.text, clear };
  if (comment.pasted !== null) params.report = comment.pasted.text;
  return (ask("board.drag_back", params) as Promise<SheetSent>).then((sent) => {
    if (sent.outcome === "done") return;
    openOverlay("confirm", {
      title: sent.title,
      body: sent.body,
      confirmLabel: sent.clear,
      cancelLabel: sent.cancel,
      destructive: false,
      onConfirm: () => sendDragBack(view, comment, true, ask),
    });
  });
}

export interface DragBackBodyProps {
  view: DragBackView;
  comment: BoxValue;
  onComment: (comment: BoxValue) => void;
  onSubmit: () => void;
  refusal: string | null;
}

/** What the agent working where the card is does, the comment box and where the card goes after. */
export function DragBackBody({ view, comment, onComment, onSubmit, refusal }: DragBackBodyProps) {
  return (
    <div className="review-body">
      {view.stops !== null && (
        <div className="review-callout" data-tone="paused">
          <Icon name="triangle-alert" size={14} />
          <div>{view.stops}</div>
        </div>
      )}
      <CommentBox
        ticket={view.key}
        view={view.comment}
        value={comment}
        onChange={onComment}
        onSubmit={onSubmit}
      />
      {view.then !== null && <p className="review-muted">{view.then}</p>}
      {refusal !== null && (
        <p className="review-error" role="alert">
          {refusal}
        </p>
      )}
    </div>
  );
}

export function DragBackDialog({ params: view, close }: OverlayProps<"drag_back">) {
  const [comment, setComment] = useState<BoxValue>(EMPTY_BOX);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const ready = boxState(comment, view.comment.limits, view.comment.optional).canSubmit;
  const confirm = () => {
    if (busy || !ready) return;
    setBusy(true);
    setRefusal(null);
    sendDragBack(view, comment).then(close, (e: unknown) => {
      setRefusal(asRequestError(e).message);
      setBusy(false);
    });
  };
  return (
    <Dialog
      title={view.title}
      width={480}
      onConfirm={confirm}
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label={view.cancel} onClick={close} />
          <Button
            icon={view.confirm_icon}
            label={view.confirm}
            loading={busy}
            disabled={!ready}
            onClick={confirm}
          />
        </>
      }
    >
      <DragBackBody
        view={view}
        comment={comment}
        onComment={setComment}
        onSubmit={confirm}
        refusal={refusal}
      />
    </Dialog>
  );
}
