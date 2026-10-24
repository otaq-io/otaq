// "Manual checks for you" (d0125, T-0499): what the agents and checks couldn't check themselves, above Approve on the
// review's Overview. Each tick goes to the service (`overview.manual_check_tick`), which records it in History and
// publishes `ticket_changed`, so the list follows; it never blocks Approve. Its words are Rust's (`ManualChecksView`).
import { useState } from "react";

import { Checkbox } from "../../components";
import type { ManualChecksView } from "../../generated/ManualChecksView";
import type { ManualCheckTickParams } from "../../generated/ManualCheckTickParams";
import { asRequestError } from "../../service";
import { request } from "../../tauri";

export function ManualChecks({ ticket, view }: { ticket: string; view: ManualChecksView }) {
  const [error, setError] = useState<string | null>(null);
  const tick = (n: number, ticked: boolean) => {
    setError(null);
    request("overview.manual_check_tick", { key: ticket, n, ticked } satisfies ManualCheckTickParams).catch(
      (e: unknown) => {
        setError(asRequestError(e).message);
      },
    );
  };
  return (
    <section className="manual-checks" aria-label={view.heading}>
      <span className="editor-section-name">{view.heading.toUpperCase()}</span>
      {view.items.map((item) => (
        <div key={item.n} className="manual-check" data-ticked={item.ticked || undefined}>
          <Checkbox
            checked={item.ticked}
            label={item.text}
            onChange={(ticked) => {
              tick(item.n, ticked);
            }}
          />
          <span className="manual-check-source">{item.source}</span>
        </div>
      ))}
      {error !== null && (
        <p className="comment-box-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
