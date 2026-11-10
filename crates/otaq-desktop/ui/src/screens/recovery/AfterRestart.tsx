// The after-restart dialog (REC-01, REC-06, d0045, d0022): opened by itself once per service start that stopped agents,
// "3 agents were stopped by a restart". One row per stopped ticket and per conversation with Noyan, ticked; a row whose
// tool can't run now says why in amber and can't be ticked, so nothing resumes on a tool that can't run; a row whose tool
// the service hasn't checked yet says "Checking Codex…" until the check answers, then updates. Resume 3
// resumes the ticked rows; each row then says what its Resume did (resumed, or couldn't, with Retry and Start from
// scratch). Not now (or Esc) closes it: the stopped tickets wait in Needs you, red with Resume. Nothing resumes without
// a click. What it says is `recovery.summary`'s (`after_restart.rs`).
import { useCallback, useEffect, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps } from "../../app/registry";
import { Button, Dialog } from "../../components";
import type { AfterRestartParams } from "../../generated/AfterRestartParams";
import type { AfterRestartRow } from "../../generated/AfterRestartRow";
import type { AfterRestartSummary } from "../../generated/AfterRestartSummary";
import type { RecoveryResumeParams } from "../../generated/RecoveryResumeParams";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { setUnchecked } from "./logic";

export interface SummaryBodyProps {
  view: AfterRestartSummary;
  /** Rows whose Resume the page is waiting for. */
  resuming: ReadonlySet<string>;
  onCheck: (id: string, checked: boolean) => void;
  onRetry: (key: string) => void;
}

/** What a resumed, resuming or failed row says in place of its detail. */
function Outcome({
  row,
  view,
  waiting,
  onRetry,
}: {
  row: AfterRestartRow;
  view: AfterRestartSummary;
  waiting: boolean;
  onRetry: (key: string) => void;
}) {
  const state = waiting ? "resuming" : row.state;
  return (
    <span className="summary-detail" data-state={state}>
      <Icon
        name={waiting ? "loader-circle" : row.state === "failed" ? "circle-x" : "circle-check"}
        size={13}
      />
      <span>{waiting ? view.resuming : row.detail}</span>
      {row.state === "failed" && !waiting && (
        <span className="summary-state-buttons">
          <Button
            compact
            variant="secondary"
            icon="rotate-ccw"
            label={view.retry}
            onClick={() => {
              onRetry(row.id);
            }}
          />
          <Button
            compact
            variant="ghost"
            label={view.scratch}
            onClick={() => {
              openOverlay("start_from_scratch", { ticket: row.id });
            }}
          />
        </span>
      )}
    </span>
  );
}

function Row({ row, view, resuming, onCheck, onRetry }: { row: AfterRestartRow } & SummaryBodyProps) {
  const waiting = resuming.has(row.id) && row.state === "stopped";
  const stopped = row.state === "stopped" && !waiting;
  return (
    <div className="summary-row" data-kind={row.kind} data-state={waiting ? "resuming" : row.state}>
      <input
        type="checkbox"
        className="checkbox"
        aria-label={`${row.tag} ${row.title}`}
        checked={row.checked && stopped}
        disabled={!stopped || row.problem !== null || row.checking !== null}
        onChange={(e) => {
          onCheck(row.id, e.target.checked);
        }}
      />
      <span className="summary-key">{row.tag}</span>
      <span className="summary-title">{row.title}</span>
      {stopped && row.checking !== null ? (
        <span className="summary-detail" data-state="checking">
          <Icon name="loader-circle" size={13} />
          <span>{row.checking}</span>
        </span>
      ) : stopped ? (
        <span className="summary-detail" data-problem={row.problem !== null || undefined}>
          {row.problem ?? row.detail}
        </span>
      ) : (
        <Outcome row={row} view={view} waiting={waiting} onRetry={onRetry} />
      )}
    </div>
  );
}

/** The dialog's body for a loaded view (also what the markup tests render). */
export function SummaryBody(props: SummaryBodyProps) {
  const { view } = props;
  return (
    <>
      <p className="summary-intro">{view.text}</p>
      <div className="summary-list">
        {view.rows.map((row) => (
          <Row key={row.id} row={row} {...props} />
        ))}
      </div>
      {view.notes.map((note) => (
        <p key={note} className="summary-note">
          {note}
        </p>
      ))}
      <p className="summary-note">{view.foot}</p>
    </>
  );
}

/** What draws the dialog again: a ticket or a conversation that changed, and the tools' states, so a row waiting for
 * its tool's check ("Checking Codex…") updates when the check answers. */
export const SUMMARY_EVENTS = ["ticket_changed", "project_agent_changed", "agent_tools_changed"];

export function AfterRestart({ close }: OverlayProps<"after_restart">) {
  const [unchecked, setUncheckedIds] = useState<ReadonlySet<string>>(new Set());
  const [resuming, setResuming] = useState<ReadonlySet<string>>(new Set());
  const load = useCallback(() => {
    const params: AfterRestartParams = { unchecked: [...unchecked] };
    return request("recovery.summary", params) as Promise<AfterRestartSummary | null>;
  }, [unchecked]);
  const { view } = useServiceView(load, SUMMARY_EVENTS);
  useEffect(() => {
    if (view === null) close();
  }, [view, close]);
  if (!view) return null;
  const resume = () => {
    const ids = [...view.selected, ...view.selected_agents];
    setResuming((r) => new Set([...r, ...ids]));
    const params: RecoveryResumeParams = { keys: view.selected, agents: view.selected_agents };
    request("recovery.resume", params)
      .catch(() => undefined)
      .finally(() => {
        setResuming((r) => {
          const next = new Set(r);
          for (const id of ids) next.delete(id);
          return next;
        });
      });
  };
  const count = view.selected.length + view.selected_agents.length;
  return (
    <Dialog
      title={view.title}
      width={600}
      hint="close"
      onCancel={close}
      onConfirm={count > 0 ? resume : undefined}
      footer={
        <>
          <Button variant="ghost" label={view.later} onClick={close} />
          <Button icon="play" label={view.resume_selected} disabled={count === 0} onClick={resume} />
        </>
      }
    >
      <SummaryBody
        view={view}
        resuming={resuming}
        onCheck={(id, checked) => {
          setUncheckedIds((u) => setUnchecked(u, [id], checked));
        }}
        onRetry={(key) => {
          request("recovery.retry", { key }).catch(() => undefined);
        }}
      />
    </Dialog>
  );
}
