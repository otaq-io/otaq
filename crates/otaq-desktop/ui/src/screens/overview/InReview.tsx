// Overview while the ticket waits in your check (`ticket-01` *The ticket in Review*): WHAT THE AGENT DID (its last
// word when its stage finished) and CHECKER (how the check went, its verdict, Full report on Changes), above the
// decisions it made and the ticket. The headings are Rust's (`overview.view`'s `in_review`); what they show is
// `files.review`'s, the notes Changes shows above All changes.
import { useCallback } from "react";

import { showTab } from "../../app/selection";
import { Button } from "../../components";
import type { FilesReviewNotes } from "../../generated/FilesReviewNotes";
import type { OverviewInReview } from "../../generated/OverviewInReview";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

const EVENTS = ["ticket_changed", "card_status_changed"] as const;

/** The checker's verdict lines, as plain text ("Every link now resolves; spelling checked."). */
function verdict(notes: FilesReviewNotes): string | null {
  const lines = notes.review?.report?.summary ?? [];
  const text = lines
    .map((line) => line.map((part) => part.text).join(""))
    .join(" ")
    .trim();
  return text === "" ? null : text;
}

export function InReview({ ticket, view }: { ticket: string; view: OverviewInReview }) {
  const load = useCallback(
    () => request("files.review", { key: ticket }) as Promise<FilesReviewNotes>,
    [ticket],
  );
  const { view: notes } = useServiceView(load, EVENTS);
  if (!notes) return null;
  const summary = notes.summary;
  const review = notes.review;
  const said = verdict(notes);
  return (
    <>
      {summary !== null && (
        <section className="in-review" aria-label={view.what_did}>
          <span className="editor-section-name">{view.what_did.toUpperCase()}</span>
          <p className="in-review-text">{summary.text}</p>
        </section>
      )}
      {review !== null && (
        <section className="in-review" aria-label={view.checker}>
          <span className="editor-section-name">{view.checker.toUpperCase()}</span>
          <div className="in-review-head">
            <span className="status-pill" data-palette={review.entry.palette}>
              <Icon name={review.entry.icon} size={12} />
              <span>{review.entry.label}</span>
            </span>
            {review.entry.text !== null && <span className="in-review-muted">· {review.entry.text}</span>}
            <span className="in-review-spacer" />
            {review.report !== null && (
              <Button
                compact
                variant="ghost"
                label={view.full_report}
                onClick={() => {
                  showTab("files");
                }}
              />
            )}
          </div>
          {said !== null && <p className="in-review-text">{said}</p>}
        </section>
      )}
    </>
  );
}
