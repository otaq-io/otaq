// The epic sheet's Lead tab (*05 → Row · Epic panel · Lead tab (0083)*, *Row · Lead tab · ended (0083)*, *16 → Lead tab
// · ended by restart · Before the restart*; LEAD-11, LEAD-12; d0083, d0045, d0048). The header holds the Lead Status,
// "read-only" and the tokens today; "Changes by the lead" lists the lead's changes by turn, each line opening its
// ticket; the body is the live terminal (typing as in a ticket's Terminal tab), Open lead (Split with the Project
// agent while the epic has no children, LEAD-13, d0091: the Project agent panel gets its first message), "Before the restart" with
// Resume, or the last terminal read-only with the ended bar. The lead's own question is the sheet header's question
// bar (`sheet.view`). A lead that waits on its tool or its model (M23) says so above the body, amber (neutral for a
// limit), with Sign in… or Change agent (*08 → Epic lead · its tool isn't available / at its limit*). Everything it
// says comes from `lead.view`; it keeps only which button is busy.
import { useCallback, useEffect, useRef, useState } from "react";

import { openTicket, terminalFocusRequests } from "../../app/selection";
import type { TicketProps } from "../../app/registry/sheet";
import { useStore } from "../../app/store";
import { Button, LeadStatus, leadAction, ToolProblemCallout } from "../../components";
import type { LeadActionKind } from "../../generated/LeadActionKind";
import type { LeadSplit } from "../../generated/LeadSplit";
import type { LeadChanges } from "../../generated/LeadChanges";
import type { LeadTabBody } from "../../generated/LeadTabBody";
import type { LeadTabLoad } from "../../generated/LeadTabLoad";
import type { LeadTabView } from "../../generated/LeadTabView";
import type { ToolProblemAction } from "../../generated/ToolProblemAction";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Terminal, type TerminalHandle } from "../../terminal/Terminal";
import { Icon } from "../../theme/Icon";
import { splitWithProjectAgent } from "../agent_panel/logic";
import { toolProblemAction } from "../tool_problems/toolProblemRunner";
import { BeforeRestart } from "../session/TerminalTab";

/** The service events that change what the tab shows. */
export const LEAD_TAB_EVENTS = [
  "lead_changed",
  "project_agent_changed",
  "ticket_changed",
  "ticket_deleted",
  "session_state",
] as const;

/** How long a focus request waits for the lead's terminal to mount. */
const FOCUS_WAIT_MS = 5000;

function Changes({ changes }: { changes: LeadChanges }) {
  const first = changes.groups.at(0);
  return (
    <section className="lead-tab-changes" aria-label={changes.title}>
      <div className="lead-tab-changes-head">
        <span className="lead-tab-changes-title">{changes.title}</span>
        <span className="lead-tab-spacer" />
        {first && <span className="lead-tab-group">{first.label}</span>}
      </div>
      {changes.groups.length === 0 && <div className="lead-tab-changes-empty">{changes.empty}</div>}
      {changes.groups.map((group, g) => (
        <div key={group.label} className="lead-tab-changes-group">
          {g > 0 && <div className="lead-tab-group">{group.label}</div>}
          {group.lines.map((line, i) => (
            <button
              key={i}
              type="button"
              className="lead-tab-change"
              aria-label={line.open_hint}
              onClick={() => {
                openTicket(line.key);
              }}
            >
              <span className="lead-tab-change-icon" data-added={line.added || undefined}>
                <Icon name={line.icon} size={12} />
              </span>
              {line.verb !== null && <span className="lead-tab-change-verb">{line.verb}</span>}
              <span className="lead-tab-change-key">{line.key}</span>
              <span className="lead-tab-change-text">{line.text}</span>
              <span className="lead-tab-change-go">
                <Icon name="arrow-up-right" size={12} />
              </span>
            </button>
          ))}
        </div>
      ))}
    </section>
  );
}

export interface LeadTabBodyProps {
  view: LeadTabView;
  terminal?: React.RefObject<TerminalHandle | null>;
  /** Open lead and Resume (the markup tests pass a stub). */
  onAction?: (kind: LeadActionKind, epic: string) => Promise<void>;
  /** Split with the Project agent on an epic with no children (the markup tests pass a stub). */
  onSplit?: (split: LeadSplit) => Promise<unknown>;
  /** A button of the waiting notice (the gallery records it; the app carries it out). */
  onToolProblem?: (action: ToolProblemAction) => void;
}

function Body({ view, body, terminal, onAction, onSplit }: LeadTabBodyProps & { body: LeadTabBody }) {
  const [busy, setBusy] = useState(false);
  const act = (kind: LeadActionKind) => {
    setBusy(true);
    void (onAction ?? leadAction)(kind, view.epic).finally(() => {
      setBusy(false);
    });
  };
  const split = (s: LeadSplit) => {
    setBusy(true);
    void (onSplit ?? ((x: LeadSplit) => splitWithProjectAgent(x, request)))(s).finally(() => {
      setBusy(false);
    });
  };
  switch (body.kind) {
    case "live":
      return (
        <div className="terminal-tab-well">
          <div className="terminal-tab-terminal">
            <Terminal
              key={body.session}
              session={body.session}
              label={body.label}
              files={{ kind: "ticket", key: view.epic }}
              ref={terminal}
            />
          </div>
        </div>
      );
    case "open_lead":
      return (
        <div className="lead-tab-open">
          <Icon name="bot" size={20} />
          <strong>{body.title}</strong>
          {body.text !== null && <p>{body.text}</p>}
          <Button
            icon={body.button.icon}
            label={body.button.label}
            loading={busy}
            onClick={() => {
              if (body.button.split) split(body.button.split);
              else act(body.button.kind);
            }}
          />
        </div>
      );
    case "loading":
      return <div className="terminal-tab-empty">{body.text}</div>;
    case "before_restart":
      return (
        <div className="terminal-tab-well">
          <BeforeRestart
            before={body}
            busy={busy}
            onResume={() => {
              act("resume");
            }}
          />
        </div>
      );
    case "not_available":
      return (
        <div className="terminal-tab-well">
          <div className="terminal-tab-recovery-gone">
            <span className="terminal-tab-muted">{body.label}</span>
            <p>{body.text}</p>
            <div className="terminal-tab-recovery-buttons">
              <Button
                compact
                icon="circle-play"
                label={body.resume}
                loading={busy}
                onClick={() => {
                  act("resume");
                }}
              />
            </div>
          </div>
        </div>
      );
    case "ended":
      return (
        <div className="terminal-tab-well">
          <div className="terminal-tab-terminal">
            {body.session !== null && (
              <Terminal
                key={body.session}
                session={body.session}
                label={body.label}
                readOnly
                files={{ kind: "ticket", key: view.epic }}
              />
            )}
          </div>
          <div className="lead-tab-ended">
            <Icon name={body.icon} size={14} />
            <div>
              <strong>{body.title}</strong>
              <span>{body.text}</span>
            </div>
            {body.resume !== null && (
              <Button
                compact
                icon="circle-play"
                label={body.resume}
                loading={busy}
                onClick={() => {
                  act("resume");
                }}
              />
            )}
          </div>
        </div>
      );
  }
}

/** The tab for a loaded view (also what the markup tests render). */
export function LeadTabBody({ view, terminal, onAction, onSplit, onToolProblem }: LeadTabBodyProps) {
  return (
    <div className="lead-tab" data-asks={view.asks_you || undefined}>
      <div className="lead-tab-head">
        <LeadStatus
          lead={view.status}
          form="full"
          onOpen={() => undefined}
          onAction={(kind, epic) => {
            void (onAction ?? leadAction)(kind, epic);
          }}
          {...(onSplit && {
            onSplit: (split: LeadSplit) => {
              void onSplit(split);
            },
          })}
        />
        {view.status.name !== null && <span className="lead-tab-muted">{view.read_only}</span>}
        <span className="lead-tab-spacer" />
        {view.tokens !== null && (
          <span className="lead-tab-tokens">
            <Icon name="coins" size={11} />
            <span>{view.tokens}</span>
          </span>
        )}
      </div>
      {view.waiting && (
        <div className="lead-tab-waiting">
          <ToolProblemCallout
            callout={view.waiting}
            look="band"
            onAction={(action) => {
              if (onToolProblem) onToolProblem(action);
              else toolProblemAction(action, { ticket: view.epic });
            }}
          />
        </div>
      )}
      {view.changes && <Changes changes={view.changes} />}
      <Body
        view={view}
        body={view.body}
        {...(terminal && { terminal })}
        {...(onAction && { onAction })}
        {...(onSplit && { onSplit })}
      />
    </div>
  );
}

/** The tab registered in the sheet: the open epic's view, and focus requests (Answer in terminal on its question). */
export function LeadTab({ ticket }: TicketProps) {
  const load = useCallback(() => request("lead.view", { key: ticket }) as Promise<LeadTabLoad>, [ticket]);
  const { view: loaded } = useServiceView(load, LEAD_TAB_EVENTS);
  const view = loaded?.view ?? null;
  const terminal = useRef<TerminalHandle | null>(null);
  const focusRequest = useStore(terminalFocusRequests);
  useEffect(() => {
    if (focusRequest?.ticket !== ticket) return;
    const deadline = Date.now() + FOCUS_WAIT_MS;
    let timer: number | undefined;
    const attempt = () => {
      if (terminal.current) {
        terminal.current.focus();
        return;
      }
      if (Date.now() < deadline) timer = window.setTimeout(attempt, 100);
    };
    attempt();
    return () => {
      window.clearTimeout(timer);
    };
  }, [focusRequest, ticket]);
  if (view === null || view.epic !== ticket) {
    return <div className="terminal-tab-empty">{loaded && loaded.view === null ? loaded.gone : null}</div>;
  }
  return <LeadTabBody view={view} terminal={terminal} />;
}
