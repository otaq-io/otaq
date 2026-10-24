// A validation round's report (VAL-12; d0063; *07 → SHOP-18 · Diff · Validated · round 2*, *Review · Failed 3 times ·
// moved by you*, *05 → History · validation rounds*): what was checked, the findings with their file and line, the
// notes with their linked keys, and Create ticket on a "Not covered by any ticket" note (the user's click). Drawn in
// the History tab under a round and on the Files tab next to the agent's summary. Every text comes from Rust
// (`otaq-view`'s `validation_report.rs`); this keeps only whether it is open and which note is being created.
import { useState } from "react";

import { openTicket, selectProject } from "../../app/selection";
import { Button } from "../../components";
import type { ReportSegment } from "../../generated/ReportSegment";
import type { ValidationReportView } from "../../generated/ValidationReportView";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

/** A key in a report opens that ticket (in its project). */
export function openKey(key: string): void {
  const project = key.split("-")[0];
  if (project) selectProject(project);
  openTicket(key, "overview");
}

function Text({ segments }: { segments: readonly ReportSegment[] }) {
  return (
    <>
      {segments.map((s, i) =>
        s.key ? (
          <button
            type="button"
            key={`${String(i)}-${s.text}`}
            className="report-key"
            onClick={() => {
              openKey(s.text);
            }}
          >
            {s.text}
          </button>
        ) : (
          <span key={`${String(i)}-${s.text}`}>{s.text}</span>
        ),
      )}
    </>
  );
}

/** The report's lines (no head): a passed one's summary or a failed one's findings, then checks and notes. `ticket` is the ticket the report is about. */
export function ReportBody({ report, ticket }: { report: ValidationReportView; ticket: string }) {
  const [creating, setCreating] = useState<string | null>(null);
  const create = (note: string) => {
    setCreating(note);
    request("sections.create_from_note", { key: ticket, note }).then(
      () => {
        setCreating(null);
      },
      () => {
        setCreating(null);
      },
    );
  };
  return (
    <div className="report-body">
      {/* A passed report's verdict ("Pass: every item… is done") reads as a pass, not as a finding (dogfood #17). */}
      {report.summary.map((line, i) => (
        <div key={`s${String(i)}`} className="report-line" data-tone="checked">
          <Icon name="check" size={12} />
          <span className="report-text">
            <span>
              <Text segments={line} />
            </span>
          </span>
        </div>
      ))}
      {report.findings.map((f, i) => (
        <div key={`f${String(i)}`} className="report-line" data-tone="finding">
          <Icon name="x" size={12} />
          <span className="report-text">
            <span>
              <Text segments={f.text} />
            </span>
            {f.place !== null && <span className="report-place">{f.place}</span>}
          </span>
        </div>
      ))}
      {report.checked.length > 0 && report.checked_heading !== null && (
        <div className="report-heading">{report.checked_heading}</div>
      )}
      {report.checked.map((c, i) => (
        <div key={`c${String(i)}`} className="report-line" data-tone="checked">
          <Icon name="check" size={12} />
          <span className="report-text">
            <span>
              <Text segments={c} />
            </span>
          </span>
        </div>
      ))}
      {report.notes.length > 0 && report.notes_heading !== null && (
        <div className="report-heading">{report.notes_heading}</div>
      )}
      {report.notes.map((n, i) => (
        <div key={`n${String(i)}`} className="report-line" data-tone="note">
          <Icon name="info" size={12} />
          <span className="report-text">
            <span>
              <Text segments={n.text} />
            </span>
          </span>
          {n.create !== null && (
            <Button
              compact
              variant="secondary"
              icon={report.create_icon}
              label={report.create}
              loading={creating === n.create}
              disabled={creating !== null}
              onClick={() => {
                if (n.create !== null) create(n.create);
              }}
            />
          )}
        </div>
      ))}
    </div>
  );
}

/** The report with its head ("Validation report · round 2 · 3 min · 11k"), collapsible; a failed one in red. */
export function Report({ report, ticket }: { report: ValidationReportView; ticket: string }) {
  const [open, setOpen] = useState(true);
  return (
    <div className="report" data-failed={report.failed || undefined}>
      <button
        type="button"
        className="report-head"
        aria-expanded={open}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <Icon name={open ? "chevron-down" : "chevron-right"} size={12} />
        <Icon name={report.failed ? "shield-x" : "shield-check"} size={13} />
        <span className="report-title">{report.title}</span>
        <span className="report-spacer" />
        <span className="report-meta">{report.meta}</span>
      </button>
      {open && <ReportBody report={report} ticket={ticket} />}
    </div>
  );
}
