// Start from scratch after a failed resume (*16 → Resume failed · Start from scratch*; REC-03, d0018): starting over
// spends tokens and drops the old conversation, so it asks first and says what is kept and what is lost. Opened by
// the Terminal tab's and the sheet's Start from scratch… and the after-restart sheet:
// `openOverlay("start_from_scratch", { ticket })`. Only the confirm starts a new `claude` (`recovery.start_from_scratch`).
import { useEffect, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { ConfirmDialog } from "../../components";
import type { ScratchConfirm } from "../../generated/ScratchConfirm";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

/** The kept and lost boxes and the History note (also what the markup tests render). */
export function ScratchBody({ view, error }: { view: ScratchConfirm; error: string | null }) {
  return (
    <>
      <p className="dialog-text">{view.text}</p>
      <div className="scratch-box" data-tone="kept">
        <strong>{view.kept_title}</strong>
        <ul>
          {view.kept.map((line) => (
            <li key={line}>
              <Icon name="check" size={13} />
              <span>{line}</span>
            </li>
          ))}
        </ul>
      </div>
      <div className="scratch-box" data-tone="lost">
        <strong>{view.lost_title}</strong>
        <ul>
          <li>
            <Icon name="x" size={13} />
            <span>{view.lost}</span>
          </li>
        </ul>
      </div>
      <p className="scratch-note">{view.note}</p>
      {error !== null && (
        <p className="cleanup-error" role="alert">
          {error}
        </p>
      )}
    </>
  );
}

export function StartFromScratch({ params, close }: OverlayProps<"start_from_scratch">) {
  const { ticket } = params;
  const [view, setView] = useState<ScratchConfirm | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  useEffect(() => {
    let live = true;
    (request("recovery.scratch", { key: ticket }) as Promise<ScratchConfirm>).then(
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
      width={480}
      body={view ? <ScratchBody view={view} error={error} /> : <p className="cleanup-error">{error}</p>}
      confirmLabel={view?.confirm ?? ""}
      confirmIcon="rotate-ccw"
      confirmDisabled={view === null || starting}
      onConfirm={() => {
        setStarting(true);
        request("recovery.start_from_scratch", { key: ticket }).then(close, (e: unknown) => {
          setStarting(false);
          setError(asRequestError(e).message);
        });
      }}
      onCancel={close}
    />
  );
}
