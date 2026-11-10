// The after-restart dialog (REC-01, REC-06, d0045, d0022): opened by itself once per service start that stopped agents,
// "3 agents were stopped by a restart". One row per stopped ticket and per conversation with Noyan, ticked; a row whose
// tool can't run now says why in amber and can't be ticked, so nothing resumes on a tool that can't run; a row whose tool
// the service hasn't checked yet says "Checking Codex…" until the check answers, then updates. A row whose tool is
// signed out has Sign in…, the same sign-in as Settings → Agents in a short terminal under the row; once it works the
// tools change, the dialog is drawn again and the row is ticked ("Resume 2" → "Resume 3"). Resume 3
// resumes the ticked rows; each row then says what its Resume did (resumed, or couldn't, with Retry and Start from
// scratch). Not now (or Esc) closes it: the stopped tickets wait in Needs you, red with Resume. Nothing resumes without
// a click. What it says is `recovery.summary`'s (`after_restart.rs`).
import { useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps } from "../../app/registry";
import { Button, Dialog } from "../../components";
import type { AfterRestartParams } from "../../generated/AfterRestartParams";
import type { AfterRestartRow } from "../../generated/AfterRestartRow";
import type { AfterRestartSummary } from "../../generated/AfterRestartSummary";
import type { AgentToolSignIn } from "../../generated/AgentToolSignIn";
import type { AgentToolsPage } from "../../generated/AgentToolsPage";
import type { RecoveryResumeParams } from "../../generated/RecoveryResumeParams";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { RowTerminal, type RowTerminals, useRowTerminals } from "../onboarding/RowTerminal";
import { agentStillOffered } from "../settings/AgentTools";
import { setUnchecked } from "./logic";

export interface SummaryBodyProps {
  view: AfterRestartSummary;
  /** Rows whose Resume the page is waiting for. */
  resuming: ReadonlySet<string>;
  onCheck: (id: string, checked: boolean) => void;
  onRetry: (key: string) => void;
  /** Sign in… on a row whose tool is signed out. */
  onSignIn?: (row: AfterRestartRow) => void;
  /** The rows' Sign in… terminal, when the dialog has one (not the markup tests). */
  terminals?: RowTerminals;
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

function Row({
  row,
  view,
  resuming,
  onCheck,
  onRetry,
  onSignIn,
  terminals,
}: { row: AfterRestartRow } & SummaryBodyProps) {
  const waiting = resuming.has(row.id) && row.state === "stopped";
  const stopped = row.state === "stopped" && !waiting;
  return (
    <>
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
            <span>{row.problem ?? row.detail}</span>
            {row.sign_in !== null && (
              <Button
                compact
                variant="secondary"
                icon="square-terminal"
                label={row.sign_in}
                onClick={() => {
                  onSignIn?.(row);
                }}
              />
            )}
          </span>
        ) : (
          <Outcome row={row} view={view} waiting={waiting} onRetry={onRetry} />
        )}
      </div>
      {terminals && <RowTerminal row={row.id} texts={view.terminal} terminals={terminals} />}
    </>
  );
}

/** What a row's Sign in… terminal waits for: the key Settings → Agents uses ("codex:sign_in"), offered until the
 * sign-in worked. */
export function signInKey(row: AfterRestartRow): string {
  return `${row.tool}:sign_in`;
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
  // The rows whose Sign in… was pressed: once their tool can run they are ticked, even one that was waiting for you.
  const [signedIn, setSignedIn] = useState<ReadonlySet<string>>(new Set());
  const load = useCallback(() => {
    const params: AfterRestartParams = { unchecked: [...unchecked], signed_in: [...signedIn] };
    return request("recovery.summary", params) as Promise<AfterRestartSummary | null>;
  }, [unchecked, signedIn]);
  const { view } = useServiceView(load, SUMMARY_EVENTS);
  // Sign in…'s terminal settles on the tools as the service checked them when it ended, as Settings → Agents does;
  // the dialog itself is drawn again on `agent_tools_changed`.
  const tools = useRef<AgentToolsPage["rows"]>([]);
  const terminals = useRowTerminals(
    (key) => agentStillOffered(tools.current, key),
    () =>
      request("agent_tools.recheck", { tool: null }).then((fresh) => {
        tools.current = (fresh as AgentToolsPage).rows;
      }),
  );
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
  const signIn = (row: AfterRestartRow) => {
    setSignedIn((s) => new Set([...s, row.id]));
    terminals.start(row.id, signInKey(row), () =>
      request("agent_tools.sign_in", { tool: row.tool }).then((opened) => opened as AgentToolSignIn),
    );
  };
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
        onSignIn={signIn}
        terminals={terminals}
      />
    </Dialog>
  );
}
