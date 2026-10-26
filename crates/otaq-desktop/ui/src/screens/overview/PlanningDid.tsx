// WHAT PLANNING DID under the Plan at a human check that reviews planning (d0129; canvas `s30-02`): one row per
// planning stage that ran, its name and agent over what it committed (and the Plan it wrote), then where the docs
// are. Its words are Rust's (`planning.rs`, `PlanningDid`).
import type { PlanningDid as PlanningDidView } from "../../generated/PlanningDid";

export function PlanningDid({ view }: { view: PlanningDidView }) {
  return (
    <section className="planning-did" aria-label={view.label}>
      <span className="editor-section-name">{view.label.toUpperCase()}</span>
      {view.rows.map((row) => (
        <div key={row.stage} className="planning-did-row">
          <div className="planning-did-head">
            <span className="planning-did-stage">{row.stage}</span>
            {row.agent !== null && <span className="planning-did-agent">· {row.agent}</span>}
          </div>
          <span className="planning-did-what">{row.what}</span>
          {row.comment !== null && <span className="planning-did-comment">{row.comment}</span>}
        </div>
      ))}
      {view.note !== null && <p className="planning-did-note">{view.note}</p>}
    </section>
  );
}
