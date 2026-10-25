// The sheet's own dialogs (CLS-01, CLS-05; d0033, d0052): *Close ticket · confirm*, which names the running agent
// and the tickets that wait for this one, and *Submit for review · dialog* with its optional note. Their texts come
// from `sheet.close_dialog` / `sheet.submit_dialog`; a refusal ("2 files are not committed…") shows inside.
import { useCallback, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { ConfirmDialog, TextArea } from "../../components";
import type { SheetCloseDialog } from "../../generated/SheetCloseDialog";
import type { SheetSubmitDialog } from "../../generated/SheetSubmitDialog";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

/** Runs a confirm once at a time; its refusal stays on screen, success closes the dialog. */
function useConfirm(close: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = (send: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    send().then(close, (e: unknown) => {
      setError(asRequestError(e).message);
      setBusy(false);
    });
  };
  return { busy, error, run };
}

function Refusal({ error }: { error: string | null }) {
  return error === null ? null : <p className="sheet-dialog-error">{error}</p>;
}

export function CloseTicketDialog({ params, close }: OverlayProps<"close_ticket">) {
  const load = useCallback(
    () => request("sheet.close_dialog", { key: params.ticket }) as Promise<SheetCloseDialog>,
    [params.ticket],
  );
  const { view } = useServiceView(load, []);
  const confirm = useConfirm(close);
  if (!view) return null;
  return (
    <ConfirmDialog
      title={view.title}
      width={480}
      destructive
      confirmLabel={view.confirm}
      confirmIcon="circle-slash"
      confirmDisabled={confirm.busy}
      body={
        <div className="sheet-dialog-body">
          {view.lines.map((line) => (
            <p key={line.text} className="sheet-dialog-line" data-agent={line.agent || undefined}>
              <Icon name={line.icon} size={13} />
              <span>{line.text}</span>
            </p>
          ))}
          <p>{view.footer}</p>
          <Refusal error={confirm.error} />
        </div>
      }
      onConfirm={() => {
        confirm.run(() => request("sheet.close", { key: params.ticket }));
      }}
      onCancel={close}
    />
  );
}

export function SubmitForReviewDialog({ params, close }: OverlayProps<"submit_for_review">) {
  const load = useCallback(
    () => request("sheet.submit_dialog", { key: params.ticket }) as Promise<SheetSubmitDialog>,
    [params.ticket],
  );
  const { view } = useServiceView(load, []);
  const [note, setNote] = useState("");
  const confirm = useConfirm(close);
  if (!view) return null;
  return (
    <ConfirmDialog
      title={view.title}
      width={480}
      confirmLabel={view.confirm}
      confirmIcon="send"
      confirmDisabled={confirm.busy}
      body={
        <div className="sheet-dialog-body">
          <p className="sheet-dialog-intro">
            <Icon name="info" size={13} />
            <span>{view.intro}</span>
          </p>
          <label className="sheet-dialog-label" htmlFor="sheet-submit-note">
            {view.note_label}
          </label>
          <TextArea
            id="sheet-submit-note"
            value={note}
            placeholder={view.note_placeholder}
            onChange={setNote}
          />
          <Refusal error={confirm.error} />
          <p>{view.footer}</p>
        </div>
      }
      onConfirm={() => {
        confirm.run(() => request("sheet.submit", { key: params.ticket, note }));
      }}
      onCancel={close}
    />
  );
}
