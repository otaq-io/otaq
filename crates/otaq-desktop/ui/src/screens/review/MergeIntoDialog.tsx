// Merge into another ticket… (the Approve menu's choice): a picker of the tickets whose branch can take this one's.
// A pick squashes this ticket's branch into the picked ticket's; the toast says whether it merged or what stopped it
// (a conflict, uncommitted files), and the dialog closes either way. Its words and rules are Rust's
// (`review.merge_into_view`, `review.merge_into`; `otaq-view`'s `approve.rs`).
import { useCallback, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { Button, Dialog } from "../../components";
import type { MergeIntoParams } from "../../generated/MergeIntoParams";
import type { MergeIntoPicker } from "../../generated/MergeIntoPicker";
import type { MergeIntoResult } from "../../generated/MergeIntoResult";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

/** The list of tickets to pick from, or why there is none. The gallery draws it too. */
export function MergeIntoBody({
  view,
  busy,
  onPick,
}: {
  view: MergeIntoPicker;
  busy: boolean;
  onPick: (key: string) => void;
}) {
  return (
    <div className="review-body">
      {view.empty !== null && <p className="review-muted">{view.empty}</p>}
      {view.items.map((item) => (
        <button
          key={item.key}
          type="button"
          className="review-choice"
          disabled={busy}
          onClick={() => {
            onPick(item.key);
          }}
        >
          <span className="review-choice-icon">
            <Icon name="git-branch" size={14} />
          </span>
          <span className="review-choice-text">
            <span className="review-choice-title">
              {item.key} {item.title}
            </span>
            <span className="review-choice-detail">{item.status}</span>
          </span>
        </button>
      ))}
      <p className="review-muted">{view.note}</p>
    </div>
  );
}

export function MergeIntoDialog({ params, close }: OverlayProps<"merge_into">) {
  const ticket = params.ticket;
  const load = useCallback(
    () => request("review.merge_into_view", { key: ticket }) as Promise<MergeIntoPicker>,
    [ticket],
  );
  const { view } = useServiceView(load, []);
  const [busy, setBusy] = useState(false);
  if (!view) return null;
  const pick = (into: string) => {
    setBusy(true);
    const merge: MergeIntoParams = { key: ticket, into };
    // Merged or refused, the toast is Rust's.
    (request("review.merge_into", merge) as Promise<MergeIntoResult>).then(close, close);
  };
  return (
    <Dialog
      title={view.title}
      width={480}
      onCancel={close}
      footer={<Button variant="ghost" label="Cancel" onClick={close} />}
    >
      <MergeIntoBody view={view} busy={busy} onPick={pick} />
    </Dialog>
  );
}
