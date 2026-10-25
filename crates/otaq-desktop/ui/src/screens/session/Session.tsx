// The sheet's Session (SHT-04; d0022, d0035, d0048, d0063; canvas `s25-06`): the switch Agent · <check's agent> · Run
// · Shell, its dots turquoise while a side waits for you and green while it runs, leading the line of the side shown:
// the agent's or the check's terminal, the Run, or the Shell. The switch and what it shows are Rust's (`SheetView.sheet`,
// `session_tab.rs`); the side shown is the selection's (`agent`, `check`, `run`, `shell`).
// A ticket whose build stage split its work (d0133; canvas `s30-03`) shows Session as a list over the terminal:
// "SESSIONS · 1 IN CHARGE · 4 HELPERS", the agent in charge first with its status, then a row per piece; a row opens
// that session's terminal ("Typing goes to 40.3's helper"). Run and Shell stay in the switch (`pieces.rs`).
import type { TicketProps } from "../../app/registry";
import { showTab } from "../../app/selection";
import { useStore } from "../../app/store";
import type { SessionListView } from "../../generated/SessionListView";
import type { SessionPartView } from "../../generated/SessionPartView";
import type { SessionRowView } from "../../generated/SessionRowView";
import { toneInk } from "../../cards/look";
import type { SheetContent } from "../../generated/SheetContent";
import { BuildAside } from "../overview/NeedsYou";
import { contentOf } from "../sheet/logic";
import { useSheet } from "../sheet/Sheet";
import { RunTab } from "./RunTab";
import { ShellTab } from "./ShellTab";
import { TerminalTab } from "./TerminalTab";
import { partShown, shownCheckAgent, shownPiece } from "./terminalLogic";

/** One row of Session's list: the agent in charge or a piece, with its status in its tone. */
function SessionRow({ row, shown, onOpen }: { row: SessionRowView; shown: boolean; onOpen: () => void }) {
  const body = (
    <>
      <span className="session-list-name">{row.name}</span>
      {row.title !== "" && <span className="session-list-title">{row.title}</span>}
      <span className="session-list-status" style={{ color: toneInk(row.tone).fg }}>
        {row.status}
      </span>
    </>
  );
  return row.opens ? (
    <button type="button" className="session-list-row" aria-pressed={shown} onClick={onOpen}>
      {body}
    </button>
  ) : (
    <div className="session-list-row" data-quiet>
      {body}
    </div>
  );
}

/**
 * Session's list (d0133, `s30-03`): the agent in charge, then a row per piece; `piece` is the terminal shown (`null`
 * the agent in charge's), `undefined` while Session shows the Run, the Shell or the check.
 */
export function SessionList({
  list,
  piece,
  onOpen,
}: {
  list: SessionListView;
  piece: string | null | undefined;
  onOpen: (piece: string | null) => void;
}) {
  return (
    <div className="session-list">
      <div className="session-list-label">{list.label}</div>
      <div className="session-list-group">{list.in_charge_label}</div>
      <SessionRow
        row={list.in_charge}
        shown={piece === null}
        onOpen={() => {
          onOpen(null);
        }}
      />
      <div className="session-list-group">{list.helpers_label}</div>
      {list.helpers.map((row) => (
        <SessionRow
          key={row.name}
          row={row}
          shown={piece === row.name}
          onOpen={() => {
            onOpen(row.name);
          }}
        />
      ))}
    </div>
  );
}

/** The switch: one button per part (one per check agent, d0130), the shown one checked. */
export function SessionSwitch({
  parts,
  shown,
  agent = null,
  label,
  onPick,
}: {
  parts: readonly SessionPartView[];
  shown: SheetContent;
  /** The check agent shown, on the check's side. */
  agent?: string | null;
  label: string;
  onPick: (content: SheetContent, agent: string | null) => void;
}) {
  return (
    <div className="segmented session-switch" role="radiogroup" aria-label={label}>
      {parts.map((part) => (
        <button
          key={`${part.content}:${part.agent ?? ""}`}
          type="button"
          role="radio"
          className="segment"
          aria-checked={partShown(part, shown, agent, parts)}
          onClick={() => {
            onPick(part.content, part.agent);
          }}
        >
          {part.label}
          {part.dot !== null && <span className="session-dot" data-dot={part.dot} />}
        </button>
      ))}
    </div>
  );
}

/** The part's body, led by the switch; Session shows only its own parts (`contentOf`), the agent's by default. */
function Part({
  ticket,
  shown,
  agent,
  piece,
  head,
}: {
  ticket: string;
  shown: SheetContent;
  agent: string | null;
  /** A helper's terminal shown from Session's list (d0133); `null` for the agent's. */
  piece: string | null;
  head: React.ReactNode;
}) {
  switch (shown) {
    case "check":
      return (
        <TerminalTab key={`check:${agent ?? ""}`} ticket={ticket} side="check" agent={agent} head={head} />
      );
    case "run":
      return <RunTab ticket={ticket} head={head} />;
    case "shell":
      return <ShellTab ticket={ticket} head={head} />;
    case "agent":
    case "overview":
    case "files":
    case "activity":
      return piece !== null ? (
        <TerminalTab key={`helper:${piece}`} ticket={ticket} side="helper" agent={piece} head={head} />
      ) : (
        <TerminalTab key="agent" ticket={ticket} side="agent" head={head} />
      );
  }
}

/** The tab registered in the sheet. */
export function SessionTab({ ticket }: TicketProps) {
  const sheet = useSheet();
  const agent = useStore(shownCheckAgent);
  const picked = useStore(shownPiece);
  const session = sheet?.view.sheet.tabs.find((t) => t.tab === "session");
  if (!sheet || !session) return null;
  const shown = contentOf(session, sheet.selected);
  const list = session.list;
  // With helpers the list names the agent in charge, so the switch keeps the check, Run and Shell (d0133).
  const parts = list === null ? session.parts : session.parts.filter((p) => p.content !== "agent");
  const piece = list !== null && list.helpers.some((h) => h.name === picked && h.opens) ? picked : null;
  const head = (
    <SessionSwitch
      parts={parts}
      shown={shown}
      agent={agent}
      label={session.label}
      onPick={(content, picked) => {
        if (picked !== null) shownCheckAgent.set(picked);
        showTab(content);
      }}
    />
  );
  // A build set aside at Review (d0125): the Run and the Shell say so at their top, and while it comes back.
  const aside = (shown === "run" || shown === "shell") && sheet.view.build_aside;
  return (
    <div className="session" data-content={shown}>
      {aside && (
        <div className="session-build-aside">
          <BuildAside line={aside} />
        </div>
      )}
      {list !== null && (
        <SessionList
          list={list}
          piece={shown === "agent" ? piece : undefined}
          onOpen={(row) => {
            shownPiece.set(row);
            showTab("agent");
          }}
        />
      )}
      <Part ticket={ticket} shown={shown} agent={agent} piece={piece} head={head} />
    </div>
  );
}
