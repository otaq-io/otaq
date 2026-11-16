// A validation round's report (VAL-12; d0063; *07 → SHOP-18 · Diff · Validated · round 2*, *Review · Failed 3 times ·
// moved by you*, *05 → History · validation rounds*): what was checked, the findings with their file and line, the
// notes with their linked keys, and Create ticket on a "Not covered by any ticket" note (the user's click). Drawn in
// the History tab under a round and on the Files tab next to the agent's summary. Every text comes from Rust
// (`otaq-view`'s `validation_report.rs`); this keeps only whether it is open and which note is being created.
//
// Each line is drawn as inline Markdown (tuq 110): the report keeps its structure (findings with their place, CHECKED,
// NOTES, Create ticket) and only the words of a line get their bold, italic, inline code and links. The keys Rust found
// in a line stay buttons, inside inline code too ("`SHOP-21`").
import { createElement, type ReactNode, useMemo, useState } from "react";

import { openTicket, selectProject } from "../../app/selection";
import { Button } from "../../components";
import type { ReportSegment } from "../../generated/ReportSegment";
import type { ValidationReportView } from "../../generated/ValidationReportView";
import { renderInlineMarkdown } from "../../markdown";
import { openInBrowser } from "../../markdown/link/url";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

/** A key in a report opens that ticket (in its project). */
export function openKey(key: string): void {
  const project = key.split("-")[0];
  if (project) selectProject(project);
  openTicket(key, "overview");
}

/** The inline elements a line's Markdown draws, drawn as themselves; anything else is a plain span. */
const INLINE = new Set(["strong", "em", "code", "s", "br"]);

/**
 * The line's keys as a pattern that finds them as whole words, the way Rust split them out (`segments`): not inside a
 * longer word ("XSHOP-21", "SHOP-210"), and a sentence's last dot isn't part of one ("SHOP-21.").
 */
function keyPattern(segments: readonly ReportSegment[]): RegExp | null {
  const keys = [...new Set(segments.filter((s) => s.key).map((s) => s.text))];
  if (keys.length === 0) return null;
  const alternatives = keys
    .sort((a, b) => b.length - a.length)
    .map((k) => k.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("|");
  // What comes before a key is matched (group 1) rather than looked behind, which older WebKit can't read.
  return new RegExp(`(^|[^A-Za-z0-9.-])(${alternatives})(?![A-Za-z0-9-]|\\.+[A-Za-z0-9-])`, "g");
}

function KeyButton({ text }: { text: string }) {
  return (
    <button
      type="button"
      className="report-key"
      onClick={() => {
        openKey(text);
      }}
    >
      {text}
    </button>
  );
}

/** A text node's words with its keys as buttons. */
function withKeys(text: string, keys: RegExp | null, at: string): ReactNode[] {
  if (keys === null) return [text];
  const out: ReactNode[] = [];
  let from = 0;
  for (const match of text.matchAll(keys)) {
    const key = match[2] ?? "";
    const start = match.index + (match[1] ?? "").length;
    if (start > from) out.push(text.slice(from, start));
    out.push(<KeyButton key={`${at}k${String(start)}`} text={key} />);
    from = start + key.length;
  }
  if (from < text.length) out.push(text.slice(from));
  return out;
}

/** A drawn node as React: text with its keys linked (not inside a link), inline elements as themselves. */
function toReact(node: ChildNode, keys: RegExp | null, at: string): ReactNode {
  if (node.nodeType === 3) return withKeys(node.textContent ?? "", keys, at);
  if (!(node instanceof Element)) return null;
  const tag = node.nodeName.toLowerCase();
  const inner = (linked: RegExp | null) =>
    [...node.childNodes].map((c, i) => toReact(c, linked, `${at}.${String(i)}`));
  if (tag === "a") {
    const href = node.getAttribute("href") ?? "";
    return (
      <a
        key={at}
        href={href}
        rel="noopener noreferrer nofollow"
        onClick={(e) => {
          e.preventDefault();
          openInBrowser(href);
        }}
        onAuxClick={(e) => {
          e.preventDefault();
        }}
      >
        {inner(null)}
      </a>
    );
  }
  return createElement(INLINE.has(tag) ? tag : "span", { key: at }, ...(tag === "br" ? [] : inner(keys)));
}

/** A report line: its words drawn as inline Markdown, its keys as buttons. */
function Text({ segments }: { segments: readonly ReportSegment[] }) {
  const drawn = useMemo(() => {
    const keys = keyPattern(segments);
    const line = segments.map((s) => s.text).join("");
    return [...renderInlineMarkdown(line, document).childNodes].map((n, i) => toReact(n, keys, String(i)));
  }, [segments]);
  return <span className="report-md">{drawn}</span>;
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
