// The sheet's Activity (SHT-05, T-0466; canvas `s25-07`; History: UI-04, REC-03, CMT-14; d0045, d0109; Q432): the
// ticket's story oldest first as a timeline under the filters All · Comments · Stages · Description. Each row: the
// time, who (you, the agent's avatar, Otaq), a plain sentence ("You created the ticket", "Grill started · Griller")
// and, where an agent ran, its duration and tokens as quiet text after it ("· 18 min · 64k tokens"). A session's start
// copies its full tool session ID (so `claude --resume <id>` finds the conversation); one entry per validation round
// (VAL-12) and the validation total. Description changes and comments open under the row: a before / after
// ("DESCRIPTION", "COMMENT"; a description edit opens by itself), or what a comment said with its report, which opens
// in the report view (a deleted comment's too). The rows, their kinds and the filters are Rust's (`sections.history`,
// `history.rs`); the page keeps the filter picked and the rows opened. Above them, the Messages between the ticket and its linked tickets or its epic's lead (MSG-01;
// the old panel's section, SHT-01, T-0469), when there are any.
import { Fragment, useCallback, useState } from "react";

import type { TicketProps } from "../../app/registry";
import { Tooltip } from "../../components";
import type { HistoryOpens } from "../../generated/HistoryOpens";
import type { HistoryView } from "../../generated/HistoryView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { Mark, ReportChip } from "../editor/CommentThread";
import { openReport } from "../editor/reportOpener";
import { MessagesSection } from "../sections/Messages";
import type { ActivityFilter } from "../../generated/ActivityFilter";
import { SECTION_EVENTS } from "../sections/logic";

const ignore = () => undefined;

function CopyId({ id, hint, copied }: { id: string; hint: string; copied: string }) {
  const [done, setDone] = useState(false);
  return (
    <Tooltip text={done ? copied : hint}>
      <button
        type="button"
        className="history-copy"
        aria-label={hint}
        onClick={() => {
          navigator.clipboard.writeText(id).then(() => {
            setDone(true);
          }, ignore);
        }}
      >
        <Icon name={done ? "check" : "copy"} size={11} />
      </button>
    </Tooltip>
  );
}

/** What a description or comment row opens: its label, "Before / after", the lines and the comment's report. */
function Opened({ opens, ticket }: { opens: HistoryOpens; ticket: string }) {
  return (
    <div className="history-opens">
      <div className="history-opens-head">
        <span className="history-opens-label">{opens.label}</span>
        {opens.caption !== null && <span className="history-opens-caption">{opens.caption}</span>}
      </div>
      {opens.lines.map((l, i) => (
        <div key={i} className="history-opens-line" data-kind={l.kind}>
          <span className="history-opens-sign">{l.sign}</span>
          <span className="history-opens-text">{l.text}</span>
        </div>
      ))}
      {opens.report !== null && (
        <ReportChip
          chip={opens.report.chip}
          onOpen={() => {
            const n = opens.report?.n;
            if (n !== undefined) openReport({ kind: "comment", ticket, n });
          }}
        />
      )}
    </div>
  );
}

/** All · Comments · Stages · Description, and "oldest first". */
function Filters({
  view,
  filter,
  onFilter,
}: {
  view: HistoryView;
  filter: ActivityFilter;
  onFilter: (filter: ActivityFilter) => void;
}) {
  return (
    <div className="activity-filters">
      <div className="segmented activity-filter" role="radiogroup" aria-label="Activity">
        {view.filters.map((f) => (
          <button
            key={f.filter}
            type="button"
            role="radio"
            className="segment"
            aria-checked={f.filter === filter}
            onClick={() => {
              onFilter(f.filter);
            }}
          >
            {f.label}
          </button>
        ))}
      </div>
      <span className="activity-order">{view.order}</span>
    </div>
  );
}

/** The rows `filter` shows: every row under All, else the rows of its kind. */
export function shownRows(view: HistoryView, filter: ActivityFilter): number[] {
  const kind = view.filters.find((f) => f.filter === filter)?.kind ?? null;
  return view.rows.flatMap((row, i) => (kind === null || row.kind === kind ? [i] : []));
}

export function HistoryBody({
  view,
  opened,
  filter: initial = "all",
}: {
  view: HistoryView;
  /** The rows open at first; by default the description edits, whose before / after is the point. */
  opened?: readonly number[];
  filter?: ActivityFilter;
}) {
  const [filter, setFilter] = useState<ActivityFilter>(initial);
  // The rows that are open (by row).
  const [open, setOpen] = useState<ReadonlySet<number>>(
    () => new Set(opened ?? view.rows.flatMap((row, i) => (row.kind === "description" ? [i] : []))),
  );
  const toggle = (i: number) => {
    const next = new Set(open);
    if (!next.delete(i)) next.add(i);
    setOpen(next);
  };
  const shown = shownRows(view, filter);
  return (
    <div className="activity-tab">
      <Filters view={view} filter={filter} onFilter={setFilter} />
      <div className="history" role="list" aria-label="Activity">
        {shown.length === 0 && <p className="history-empty">{view.empty}</p>}
        {shown.map((i) => {
          const row = view.rows[i];
          if (row === undefined) return null;
          const sentence = (
            <>
              {row.title}
              {row.cost !== null && <span className="history-cost"> {row.cost}</span>}
            </>
          );
          return (
            <Fragment key={`${row.when}-${row.title}-${String(i)}`}>
              <div className="history-row" role="listitem" data-opens={row.opens !== null || undefined}>
                <span className="history-when">{row.when}</span>
                <span className="history-icon history-mark">
                  <Mark row={{ mark: row.mark }} />
                </span>
                <span className="history-event">
                  {row.opens !== null ? (
                    <button
                      type="button"
                      className="history-title history-round"
                      aria-expanded={open.has(i)}
                      onClick={() => {
                        toggle(i);
                      }}
                    >
                      <span>{sentence}</span>
                      <Icon name={open.has(i) ? "chevron-down" : "chevron-right"} size={11} />
                    </button>
                  ) : (
                    <span className="history-title">{sentence}</span>
                  )}
                  {row.detail !== null && (
                    <span className="history-detail">
                      <span className="history-detail-text">{row.detail}</span>
                      {row.tool_session_id !== null && (
                        <CopyId id={row.tool_session_id} hint={view.copy_hint} copied={view.copied} />
                      )}
                    </span>
                  )}
                </span>
              </div>
              {row.opens !== null && open.has(i) && (
                <div className="history-report">
                  <Opened opens={row.opens} ticket={view.key} />
                </div>
              )}
            </Fragment>
          );
        })}
        {view.total !== null && (
          <div className="history-total">
            <span>{view.total.label}</span>
            <span className="history-total-tokens">{view.total.tokens}</span>
          </div>
        )}
      </div>
    </div>
  );
}

/** The tab registered in the sheet. */
export function ActivityTab({ ticket }: TicketProps) {
  const load = useCallback(
    () => request("sections.history", { key: ticket }) as Promise<HistoryView | null>,
    [ticket],
  );
  const { view } = useServiceView(load, SECTION_EVENTS);
  return (
    <>
      <div className="activity-messages">
        <MessagesSection ticket={ticket} />
      </div>
      {view && <HistoryBody view={view} />}
    </>
  );
}
