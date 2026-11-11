// The Project agent panel (T-0330, T-0458; SHL-04, SHB-03, PAG-01…PAG-07, ATT-03; d0113, d0087, d0068, d0060, d0077;
// *18 · Project agents & terminals → Row · Pane: empty, hidden, tool problems, menu (0087)*, *Row · More pane states
// (0086 · V)*, *Row · Switching agents, attention (0086 · V)*, *24 → Row · The Project agent*): the `agent_panel` slot, 420 px
// (or as dragged, T-0502) between the rail and the main area while open. It shows one conversation, or the empty panel
// "Ask Noyan" whose first message starts one (d0087). Its header has "Noyan", the switcher (the
// conversation ▾), the state pill, …, Recently ended, +, Expand (T-0502) and the close icon (a narrow panel folds
// Recently ended and + into … and shows the pill's icon only, `agent_panel.css`); then the agent line,
// Created and changed, its real terminal (T-0216's component, `terminal.attach`) and the footer. The close icon, ⌘L
// and Esc (outside the terminal) close it. Everything it says comes from `agent_panel.view`.
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { shellLayout, useSizeClass, useWindowWidth } from "../../app/layout";
import { openOverlay } from "../../app/overlays";
import { useShortcut } from "../../app/registry";
import { openTicket, useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import {
  AnchoredMenu,
  Button,
  Kbd,
  Popover,
  StatusPill,
  ToolAvatar,
  ToolProblemCallout,
  Tooltip,
  PLAIN_TEXT,
  Select,
} from "../../components";
import type { AgentCloseDialog } from "../../generated/AgentCloseDialog";
import type { AgentPanelAgent } from "../../generated/AgentPanelAgent";
import type { AgentPanelAgentPick } from "../../generated/AgentPanelAgentPick";
import type { AgentPanelAgentLine } from "../../generated/AgentPanelAgentLine";
import type { AgentPanelButton } from "../../generated/AgentPanelButton";
import type { AgentPanelCreated } from "../../generated/AgentPanelCreated";
import type { AgentPanelCreatedRow } from "../../generated/AgentPanelCreatedRow";
import type { AgentPanelEmpty } from "../../generated/AgentPanelEmpty";
import type { AgentPanelEnded } from "../../generated/AgentPanelEnded";
import type { AgentPanelParams } from "../../generated/AgentPanelParams";
import type { AgentPanelSwitcher } from "../../generated/AgentPanelSwitcher";
import type { AgentPanelSwitcherRow } from "../../generated/AgentPanelSwitcherRow";
import type { AgentPanelUndoParams } from "../../generated/AgentPanelUndoParams";
import type { AgentPanelView } from "../../generated/AgentPanelView";
import type { RecentAgents } from "../../generated/RecentAgents";
import type { ToolProblemCallout as ToolProblemCalloutView } from "../../generated/ToolProblemCallout";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Terminal, type TerminalHandle } from "../../terminal/Terminal";
import { AgentQuestionBox } from "./AgentQuestion";
import { Icon } from "../../theme/Icon";
import { startTicket } from "../board/Board";
import { showAgentOnBoard } from "../board/filter";
import { toolProblemAction } from "../tool_problems/toolProblemRunner";
import {
  chosenAgent,
  closeAgentPanel,
  inPanel,
  focusFits,
  focusTarget,
  foldedAway,
  hasChoice,
  leaveExpand,
  lookedAt,
  moreMenu,
  NEW_ITEM,
  newProjectAgent,
  pickStartOn,
  type AgentPanelFocusRequest,
  agentPanelDrafts,
  agentPanelFocusRequests,
  sendFirstMessage,
  showAgent,
  shownAgents,
  startOn,
  takeAgentPanelDraft,
  toggleExpandAgentPanel,
} from "./logic";
import { CloseAgentDialog, RecentList } from "./parts";
import { optionItems } from "../settings/Agents";

/** The service events that change what the agent panel shows. */
export const AGENT_PANEL_EVENTS = [
  "project_agent_changed",
  "ticket_changed",
  "ticket_deleted",
  "projects_changed",
  // The user's keys in its tooltips (KEY-08).
  "settings_changed",
] as const;

const ignore = () => undefined;

/**
 * The newest focus request the live agent panel carried out. Kept outside the component: a request from another project's
 * strip item lands in the agent panel mounted for that project.
 */
const focusHandled: { last: AgentPanelFocusRequest | null } = { last: null };

/** A header button with its tooltip (and keys); one that `folds` goes into the … menu on a narrow panel. */
function HeaderButton({
  button,
  open,
  folds,
  onClick,
  ref,
}: {
  button: AgentPanelButton;
  open?: boolean;
  folds?: boolean;
  onClick: () => void;
  ref?: React.Ref<HTMLButtonElement>;
}) {
  return (
    <Tooltip text={button.label} keys={button.keys ?? undefined}>
      <button
        ref={ref}
        type="button"
        className="agent-panel-icon-button"
        aria-label={button.label}
        aria-expanded={open}
        data-open={open || undefined}
        data-folds={folds || undefined}
        onClick={onClick}
      >
        <Icon name={button.icon} size={15} />
      </button>
    </Tooltip>
  );
}

/** The line under the header: the tool avatar (hidden with one tool), the named agent, the tokens or "starts when you send". */
function AgentLine({ line, onRename }: { line: AgentPanelAgentLine; onRename?: () => void }) {
  return (
    <div className="agent-panel-line">
      {line.avatar !== null && <ToolAvatar avatar={line.avatar} />}
      <span className="agent-panel-line-tool">{line.agent}</span>
      <span className="agent-panel-line-note" data-tokens={line.rename !== null || undefined}>
        {line.note}
      </span>
      <span className="agent-panel-spacer" />
      <span className="agent-panel-line-reads">{line.reads}</span>
      {line.rename !== null && (
        <button type="button" className="agent-panel-rename" onClick={onRename}>
          <Icon name="pencil" size={11} />
          <span>{line.rename}</span>
        </button>
      )}
    </div>
  );
}

/** A tool problem's notice under the agent line (M23, SHL-04): amber for a tool that isn't available, neutral for a
 * limit, with Sign in… or Resume after reset; the conversation waits for its tool, and no other agent is offered. */
function Notice({ notice, project }: { notice: ToolProblemCalloutView; project: string }) {
  return (
    <div className="agent-panel-notice">
      <ToolProblemCallout
        callout={notice}
        look="band"
        onAction={(action) => {
          toolProblemAction(action, { project });
        }}
      />
    </div>
  );
}

/** One conversation in the switcher: a click shows it; Resume on an ended one continues it. */
function SwitcherItem({
  row,
  onPick,
  onResume,
}: {
  row: AgentPanelSwitcherRow;
  onPick: (agent: string) => void;
  onResume: (agent: string) => void;
}) {
  return (
    <div className="agent-panel-switcher-item">
      <button
        type="button"
        role="menuitem"
        className="agent-panel-switcher-row"
        data-selected={row.selected || undefined}
        data-palette={row.pill.palette}
        onClick={() => {
          onPick(row.id);
        }}
      >
        <Icon name="bot" size={14} />
        <span className="agent-panel-switcher-name" data-unnamed={row.unnamed || undefined}>
          {row.name}
        </span>
        {row.avatar !== null && <ToolAvatar avatar={row.avatar} />}
        <span className="agent-panel-switcher-state">{row.pill.label}</span>
      </button>
      {row.resume !== null && (
        <button
          type="button"
          role="menuitem"
          className="agent-panel-switcher-resume"
          aria-label={`${row.resume} ${row.name}`}
          onClick={() => {
            onResume(row.id);
          }}
        >
          <Icon name="play" size={12} />
          <span>{row.resume}</span>
        </button>
      )}
    </div>
  );
}

/** The switcher's list (`h038v`): the project's open conversations with their states, the recently ended ones (each
 * continues with Resume), then the empty agent panel. */
export function SwitcherList({
  switcher,
  onPick,
  onResume,
}: {
  switcher: AgentPanelSwitcher;
  onPick: (agent: string | null) => void;
  onResume: (agent: string) => void;
}) {
  return (
    <div className="agent-panel-switcher" role="menu">
      {switcher.rows.map((row) => (
        <SwitcherItem key={row.id} row={row} onPick={onPick} onResume={onResume} />
      ))}
      {switcher.recent.length > 0 && (
        <>
          <span className="agent-panel-caps agent-panel-switcher-caps">{switcher.recent_title}</span>
          {switcher.recent.map((row) => (
            // A closed conversation has nothing to show until it continues: a click resumes it too.
            <SwitcherItem key={row.id} row={row} onPick={onResume} onResume={onResume} />
          ))}
        </>
      )}
      <button
        type="button"
        role="menuitem"
        className="agent-panel-switcher-row agent-panel-switcher-new"
        onClick={() => {
          onPick(null);
        }}
      >
        <Icon name="plus" size={14} />
        <span className="agent-panel-switcher-name">{switcher.new_label}</span>
        {switcher.new_keys !== null && <Kbd keys={switcher.new_keys} />}
      </button>
    </div>
  );
}

/** The name, editable in place (Rename, a double-click); Enter keeps it, Esc leaves it. */
function NameField({
  name,
  label,
  onRename,
  onDone,
}: {
  name: string;
  label: string;
  onRename: (name: string) => void;
  onDone: () => void;
}) {
  const [draft, setDraft] = useState(name);
  return (
    <input
      {...PLAIN_TEXT}
      className="agent-panel-name-input"
      aria-label={label}
      value={draft}
      // eslint-disable-next-line jsx-a11y/no-autofocus -- Rename puts the cursor in the name, as `FIQbN` shows
      autoFocus
      onChange={(e) => {
        setDraft(e.target.value);
      }}
      onBlur={onDone}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          onRename(draft);
          onDone();
        } else if (e.key === "Escape") {
          e.stopPropagation();
          onDone();
        }
      }}
    />
  );
}

/** One created or changed ticket: its key (opens it), title, then Start while it is in Backlog (only the user starts
 * a ticket; `board.start`, as a Backlog card's Start), else where it is and what its card says; Delete while it never
 * started. A changed row has Undo while that part of the ticket still reads as Noyan left it. */
function CreatedRow({
  row,
  onStart,
  onUndo,
}: {
  row: AgentPanelCreatedRow;
  onStart: (key: string) => void;
  onUndo?: (row: AgentPanelCreatedRow) => void;
}) {
  return (
    <div className="agent-panel-created-row" data-edited={row.edited || undefined}>
      <Icon name={row.icon} size={13} />
      {row.undone !== null && (
        // An undone change reads "Undone · SHOP-7 no longer waits for SHOP-6".
        <span className="agent-panel-created-undone">{`${row.undone} ·`}</span>
      )}
      <button
        type="button"
        className="agent-panel-created-key"
        onClick={() => {
          openTicket(row.key);
        }}
      >
        {row.key}
      </button>
      <span className="agent-panel-created-text">
        {row.change !== null ? (
          // A changed row says what changed ("SHOP-7 · now waits for SHOP-6"), a created one what it was given
          // ("SHOP-16 · waits for SHOP-15"); the title is its tooltip.
          <span className="agent-panel-created-title" title={row.title || undefined}>
            {row.undone !== null ? row.change : `· ${row.change}`}
          </span>
        ) : (
          <span className="agent-panel-created-title">{row.title}</span>
        )}
        {row.line !== null && <span className="agent-panel-created-line">{row.line}</span>}
      </span>
      {row.place !== "" && <span className="agent-panel-created-place">{row.place}</span>}
      {row.delete !== null && (
        <Tooltip text={row.delete}>
          <button
            type="button"
            className="agent-panel-created-delete"
            aria-label={`${row.delete} ${row.key}`}
            onClick={() => {
              openOverlay("delete_ticket", { ticket: row.key });
            }}
          >
            <Icon name="trash-2" size={13} />
          </button>
        </Tooltip>
      )}
      {row.start !== null && (
        <Button
          compact
          variant="secondary"
          icon="play"
          label={row.start}
          onClick={() => {
            onStart(row.key);
          }}
        />
      )}
      {row.undo !== null && onUndo !== undefined && (
        <Button
          compact
          variant="secondary"
          icon="undo-2"
          label={row.undo}
          onClick={() => {
            onUndo(row);
          }}
        />
      )}
    </div>
  );
}

/** Created and changed (collapsible; folded to one line at the 1280 class): the created tickets on top with Start,
 * then the changed ones; Show on board. */
function CreatedBox({
  created,
  open,
  onToggle,
  onShowOnBoard,
  onStart,
  onUndo,
}: {
  created: AgentPanelCreated;
  open: boolean;
  onToggle: () => void;
  onShowOnBoard: () => void;
  onStart: (key: string) => void;
  onUndo: (row: AgentPanelCreatedRow) => void;
}) {
  // Open, the head names the created tickets ("CREATED 2 · IN BACKLOG"), or the changed ones when none was created.
  const head = open ? (created.title ?? created.changed_title ?? created.line) : created.line;
  const changedTitle = created.title !== null ? created.changed_title : null;
  return (
    <div className="agent-panel-created" data-open={open || undefined}>
      <div className="agent-panel-created-head">
        <button type="button" className="agent-panel-created-toggle" aria-expanded={open} onClick={onToggle}>
          <Icon name={open ? "chevron-down" : "chevron-right"} size={12} />
          <span>{head}</span>
        </button>
        <span className="agent-panel-spacer" />
        <button type="button" className="agent-panel-link" onClick={onShowOnBoard}>
          {created.show_on_board.label}
        </button>
      </div>
      {open && (
        <>
          {created.rows.map((row) => (
            <CreatedRow key={row.key} row={row} onStart={onStart} />
          ))}
          {changedTitle !== null && <span className="agent-panel-caps">{changedTitle}</span>}
          {created.changed.map((row) => (
            // One row per change: a ticket can have several, each saying something else.
            <CreatedRow
              key={`${row.key}-changed-${row.change ?? ""}`}
              row={row}
              onStart={onStart}
              onUndo={onUndo}
            />
          ))}
        </>
      )}
    </div>
  );
}

/** The new conversation's own pick of agent: the agent Noyan uses unless another is picked here, for this conversation
 * only. */
function AgentPick({ pick, project }: { pick: AgentPanelAgentPick; project: string }) {
  const current = pick.options.find((o) => o.selected);
  return (
    <Select
      label={pick.label}
      value={current?.label ?? pick.label}
      items={optionItems(pick.options)}
      onChange={(id) => {
        pickStartOn(project, id, pick.usual);
      }}
    />
  );
}

/** The empty agent panel (`SbFln`): what it will start, the examples, Recently ended and the first-message box. */
function EmptyState({
  empty,
  recent,
  draft,
  onDraft,
  onSend,
  onResume,
  box,
  project,
}: {
  empty: AgentPanelEmpty;
  project: string;
  recent: RecentAgents;
  draft: string;
  onDraft: (text: string) => void;
  onSend: () => void;
  onResume: (id: string) => void;
  box: React.RefObject<HTMLTextAreaElement | null>;
}) {
  return (
    <>
      <AgentLine line={empty.line} />
      {empty.notice !== null && <Notice notice={empty.notice} project={project} />}
      <div className="agent-panel-empty">
        <div className="agent-panel-hello">
          <h2 className="agent-panel-hello-title">{empty.title}</h2>
          <p className="agent-panel-hello-text">{empty.text}</p>
        </div>
        <div className="agent-panel-examples">
          {empty.examples.map((example) => (
            <button
              key={example.label}
              type="button"
              className="agent-panel-example"
              onClick={() => {
                // A click fills the box and never sends (d0087).
                onDraft(example.fill);
                box.current?.focus();
              }}
            >
              <Icon name="corner-down-right" size={12} />
              <span>{example.label}</span>
            </button>
          ))}
        </div>
        {recent.rows.length > 0 && (
          <div className="agent-panel-ended-list">
            <span className="agent-panel-caps">{empty.recent_title}</span>
            {recent.rows.map((row) => (
              <div key={row.id} className="agent-panel-ended-row">
                <Icon name="bot" size={13} />
                <span className="agent-panel-ended-name">{row.name}</span>
                <span className="agent-panel-ended-when">{row.ended}</span>
                <span className="agent-panel-spacer" />
                <button
                  type="button"
                  className="agent-panel-link"
                  onClick={() => {
                    onResume(row.id);
                  }}
                >
                  {row.resume}
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="agent-panel-composer">
        <div className="agent-panel-input">
          <textarea
            {...PLAIN_TEXT}
            ref={box}
            className="agent-panel-box"
            rows={2}
            aria-label={empty.placeholder}
            placeholder={empty.placeholder}
            value={draft}
            onChange={(e) => {
              onDraft(e.target.value);
            }}
            onKeyDown={(e) => {
              // Esc leaves Expand, else folds the panel (the box keeps its words, d0113); Enter sends, Shift+Enter is a
              // new line.
              if (e.key === "Escape") {
                e.preventDefault();
                if (!leaveExpand()) closeAgentPanel();
                return;
              }
              if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
              e.preventDefault();
              onSend();
            }}
          />
          <div className="agent-panel-input-row">
            {empty.pick !== null && <AgentPick pick={empty.pick} project={project} />}
            <span className="agent-panel-input-hint">{empty.hint}</span>
            <span className="agent-panel-spacer" />
            <button
              type="button"
              className="agent-panel-send"
              aria-label={empty.send}
              disabled={draft.trim() === ""}
              onClick={onSend}
            >
              <Icon name="arrow-up" size={14} />
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

/** The shown agent: its line, Created and changed, its terminal and the footer. */
function RunningAgent({
  agent,
  name,
  createdOpen,
  onToggleCreated,
  onShowOnBoard,
  onRename,
  onResume,
  terminal,
  project,
}: {
  agent: AgentPanelAgent;
  project: string;
  name: string;
  createdOpen: boolean;
  onToggleCreated: () => void;
  onShowOnBoard: () => void;
  onRename: () => void;
  onResume: () => void;
  terminal: React.RefObject<TerminalHandle | null>;
}) {
  const { footer } = agent;
  return (
    <>
      <AgentLine line={agent.line} onRename={onRename} />
      {agent.notice !== null && <Notice notice={agent.notice} project={project} />}
      {agent.created !== null && (
        <CreatedBox
          created={agent.created}
          open={createdOpen}
          onToggle={onToggleCreated}
          onShowOnBoard={onShowOnBoard}
          onStart={(key) => {
            void startTicket(key);
          }}
          onUndo={(row) => {
            if (row.change !== null && agent.created !== null) {
              const params: AgentPanelUndoParams = {
                agent: agent.created.show_on_board.agent,
                ticket: row.key,
                change: row.change,
              };
              request("agent_panel.undo", params).catch(ignore);
            }
          }}
        />
      )}
      {agent.question !== null && !agent.ended && (
        <div className="agent-panel-question">
          <AgentQuestionBox
            key={agent.question.answer.question}
            question={agent.question}
            onTerminal={() => {
              terminal.current?.focus();
            }}
          />
        </div>
      )}
      <div className="agent-panel-well">
        <div className="agent-panel-terminal">
          {agent.session !== null && (
            <Terminal
              key={agent.session}
              session={agent.session}
              label={`${name} · ${agent.line.agent}`}
              readOnly={agent.ended}
              files={{ kind: "project", project }}
              ref={terminal}
            />
          )}
        </div>
      </div>
      <div className="agent-panel-footer" data-ended={agent.ended || undefined}>
        <Icon name={agent.ended ? "circle-stop" : "keyboard"} size={12} />
        <span className="agent-panel-footer-text">{footer.text}</span>
        <span className="agent-panel-spacer" />
        {footer.keys !== null && <Kbd keys={footer.keys} />}
        <span className="agent-panel-footer-label">{footer.keys_label}</span>
      </div>
      {footer.resume !== null && (
        <div className="agent-panel-resume">
          <Button
            compact
            variant={footer.resume.primary ? "primary" : "secondary"}
            icon="play"
            label={footer.resume.label}
            onClick={onResume}
          />
          {footer.resume_note !== null && (
            <span className="agent-panel-resume-note">{footer.resume_note}</span>
          )}
        </div>
      )}
    </>
  );
}

type Floating = "switcher" | "menu" | "recent" | null;

export interface AgentPanelBodyProps {
  view: AgentPanelView;
  project: string;
  /** Follows ⌘L, ⇧⌘L and the strip's focus requests (the live agent panel; not the gallery's copies). */
  followFocus?: boolean;
}

/** The agent panel for a loaded view (also what the gallery and the tests render). */
export function AgentPanelBody({ view, project, followFocus = false }: AgentPanelBodyProps) {
  const { header, running } = view;
  const expanded = useStore(shellLayout).agentExpanded;
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const [floating, setFloating] = useState<Floating>(null);
  // The narrow header folded Recently ended and + into the … menu (read as the menu opens): Recently ended then opens
  // from the … button.
  const [folded, setFolded] = useState(false);
  const [dialog, setDialog] = useState<AgentCloseDialog | null>(null);
  // The user's own fold of Created and changed, per agent; else the view's (folded at the 1280 class).
  const [createdOpen, setCreatedOpen] = useState<Readonly<Record<string, boolean>>>({});
  const nameButton = useRef<HTMLButtonElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const recentButton = useRef<HTMLButtonElement>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const terminal = useRef<TerminalHandle>(null);
  const asked = useStore(agentPanelFocusRequests);
  const target = focusTarget(view);

  // ⌘L, ⇧⌘L, Send and Needs you ask for the keyboard: once per request, when the agent panel shows what it was asked for.
  useEffect(() => {
    if (!followFocus || !asked || asked === focusHandled.last || !focusFits(asked, view.agent)) return;
    if (target === "none") return;
    focusHandled.last = asked;
    if (target === "terminal") terminal.current?.focus();
    else box.current?.focus();
  }, [followFocus, asked, view.agent, target]);

  // Ask Noyan about this (a ticket's ⋯ menu): the empty panel's box takes the words addressed to the ticket, the caret
  // after them. Nothing is sent until the user does.
  const drafts = useStore(agentPanelDrafts);
  useEffect(() => {
    if (!followFocus || view.agent !== null || drafts[project] === undefined) return;
    const text = takeAgentPanelDraft(project);
    if (text === null) return;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the words arrive from outside React (a store), once
    setDraft(text);
    requestAnimationFrame(() => {
      const el = box.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(text.length, text.length);
    });
  }, [followFocus, view.agent, drafts, project]);

  const close = () => {
    setFloating(null);
  };
  const resume = (id: string) => {
    close();
    request("agent_panel.resume", { id }).then((shown) => {
      showAgent(project, shown as string);
    }, ignore);
  };
  const rename = (name: string) => {
    if (view.agent !== null) request("agent_panel.rename", { id: view.agent, name }).catch(ignore);
  };
  const end = (confirm: boolean) => {
    if (view.agent === null) return;
    request("agent_panel.end", { project, id: view.agent, confirm }).then((ended) => {
      const outcome = ended as AgentPanelEnded;
      setDialog(outcome.outcome === "ask" ? outcome.dialog : null);
    }, ignore);
  };
  const send = () => {
    sendFirstMessage(project, draft, request, undefined, view.empty?.starts_on ?? null).then((id) => {
      if (id !== null) setDraft("");
    }, ignore);
  };
  const choose = (item: string) => {
    close();
    switch (item) {
      case "rename":
        setEditing(true);
        break;
      case "end":
        end(false);
        break;
      case "close_ended":
        request("agent_panel.close_ended", { project }).catch(ignore);
        break;
      case "recent":
        setFloating("recent");
        break;
      case NEW_ITEM:
        newProjectAgent();
        break;
      // The menu's ids are `agent_panel.rs`'s; another one does nothing here.
      default:
        break;
    }
  };
  const created = running?.created ?? null;
  const open = view.agent !== null && created !== null ? (createdOpen[view.agent] ?? !created.folded) : false;

  let title: ReactNode = (
    <span
      className="agent-panel-title"
      data-unnamed={header.unnamed || undefined}
      data-empty={running === null || undefined}
    >
      {header.title}
    </span>
  );
  if (editing && view.agent !== null) {
    title = (
      <NameField
        name={header.title}
        label={running?.line.rename ?? header.title}
        onRename={rename}
        onDone={() => {
          setEditing(false);
        }}
      />
    );
  } else if (header.switcher) {
    title = (
      <Tooltip text={header.title}>
        <button
          ref={nameButton}
          type="button"
          className="agent-panel-switch"
          aria-haspopup="menu"
          aria-expanded={floating === "switcher"}
          onClick={() => {
            setFloating(floating === "switcher" ? null : "switcher");
          }}
          onDoubleClick={() => {
            if (view.agent !== null) setEditing(true);
          }}
        >
          {title}
          <Icon name="chevron-down" size={13} />
        </button>
      </Tooltip>
    );
  }

  return (
    <section className="agent-panel" aria-label={header.label} data-agent={view.agent ?? undefined}>
      <div className="agent-panel-header">
        <span className="agent-panel-bot" data-running={running !== null || undefined}>
          <Icon name={header.icon} size={15} />
        </span>
        <span className="agent-panel-label">{header.label}</span>
        {title}
        {header.pill !== null && (
          // A narrow panel shows only its icon; the words stay for screen readers and in the tooltip.
          <Tooltip text={header.pill.label}>
            <StatusPill pill={header.pill} />
          </Tooltip>
        )}
        <span className="agent-panel-spacer" />
        <HeaderButton
          ref={moreButton}
          button={header.more}
          open={floating === "menu"}
          onClick={() => {
            setFolded(foldedAway(recentButton.current));
            setFloating(floating === "menu" ? null : "menu");
          }}
        />
        <HeaderButton
          ref={recentButton}
          button={header.recent}
          open={floating === "recent"}
          folds
          onClick={() => {
            setFolded(false);
            setFloating(floating === "recent" ? null : "recent");
          }}
        />
        {running !== null && (
          <HeaderButton
            button={header.new}
            folds
            onClick={() => {
              newProjectAgent();
            }}
          />
        )}
        <HeaderButton
          button={expanded ? header.collapse : header.expand}
          onClick={() => {
            toggleExpandAgentPanel();
          }}
        />
        <HeaderButton
          button={header.close}
          onClick={() => {
            closeAgentPanel();
          }}
        />
      </div>
      {running === null && view.empty !== null && (
        <EmptyState
          empty={view.empty}
          recent={view.recent}
          draft={draft}
          onDraft={setDraft}
          onSend={send}
          onResume={resume}
          box={box}
          project={project}
        />
      )}
      {running !== null && view.agent !== null && (
        <RunningAgent
          agent={running}
          name={header.title}
          createdOpen={open}
          onToggleCreated={() => {
            const agent = view.agent;
            if (agent !== null) setCreatedOpen({ ...createdOpen, [agent]: !open });
          }}
          onShowOnBoard={() => {
            if (created === null) return;
            showAgentOnBoard(project, created.show_on_board.agent);
          }}
          onRename={() => {
            setEditing(true);
          }}
          onResume={() => {
            if (view.agent !== null) resume(view.agent);
          }}
          terminal={terminal}
          project={project}
        />
      )}
      {floating === "switcher" && (
        <Popover label={header.title} anchor={nameButton} align="start" onClose={close}>
          <SwitcherList
            switcher={view.switcher}
            onPick={(agent) => {
              close();
              if (agent === null) newProjectAgent();
              else showAgent(project, agent);
            }}
            onResume={resume}
          />
        </Popover>
      )}
      {floating === "menu" && (
        <AnchoredMenu
          anchor={moreButton}
          align="end"
          label={header.more.label}
          items={moreMenu(view.menu, header.new, folded, running !== null)}
          width={260}
          onChoose={choose}
          onClose={close}
        />
      )}
      {floating === "recent" && (
        <Popover
          label={header.recent.label}
          anchor={folded ? moreButton : recentButton}
          align="end"
          onClose={close}
        >
          <RecentList recent={view.recent} onResume={resume} />
        </Popover>
      )}
      {dialog !== null && (
        <CloseAgentDialog
          dialog={dialog}
          onConfirm={() => {
            setDialog(null);
            end(true);
          }}
          onCancel={() => {
            setDialog(null);
          }}
        />
      )}
    </section>
  );
}

/**
 * Esc folds the open panel while the keyboard is in it (d0113); in its terminal Esc is the agent's own (0097 binds no
 * Esc there), elsewhere it is the next taker's (the ticket's Close), and the box keeps Esc for itself.
 */
function foldFromInside(): boolean {
  if (!shellLayout.get().agentOpen || !inPanel(document.activeElement)) return false;
  closeAgentPanel();
  return true;
}

/** Esc while expanded (`leaveExpand`), as a shortcut handler. */
function leaveExpandKey(): boolean {
  return leaveExpand();
}

/** One project's agent panel: asks `agent_panel.view` for the agent the page shows, and opens on `opens_on` the first time. */
function ProjectAgentPanel({ project }: { project: string }) {
  const shown = useStore(shownAgents);
  const starts = useStore(startOn);
  const chosen = chosenAgent(shown, project);
  // The new conversation's own pick: the empty agent panel names that agent and its first message starts it.
  const startsOn = chosen === null ? starts[project] : undefined;
  const chose = hasChoice(shown, project);
  const width = useWindowWidth();
  const cls = useSizeClass();
  // Created and changed folds by the window's size class: ask again when the class changes, not on every pixel.
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the width is read when the class changes
  const windowWidth = useMemo(() => width, [cls]);
  const load = useCallback(() => {
    const params: AgentPanelParams = {
      project,
      agent: chosen,
      window_width: windowWidth,
      ...(startsOn !== undefined && { start_on: startsOn }),
    };
    return request("agent_panel.view", params) as Promise<AgentPanelView>;
  }, [project, chosen, windowWidth, startsOn]);
  const { view } = useServiceView(load, AGENT_PANEL_EVENTS);
  useEffect(() => {
    if (!chose && view?.opens_on != null) showAgent(project, view.opens_on);
  }, [chose, view?.opens_on, project]);
  useShortcut("close_sheet", foldFromInside);
  // Added after the fold, so tried before it: Esc leaves Expand first.
  useShortcut("close_sheet", leaveExpandKey);
  // The user looks at the shown conversation while the panel is open, until it closes or shows another: its replies
  // until then aren't new on the rail's Noyan button (SHB-03).
  const open = useStore(shellLayout).agentOpen;
  const shownAgent = view?.agent ?? null;
  useEffect(() => {
    if (!open || shownAgent === null) return;
    lookedAt(shownAgent);
    return () => {
      lookedAt(shownAgent);
    };
  }, [open, shownAgent]);
  if (!view) return <section className="agent-panel" />;
  return <AgentPanelBody view={view} project={project} followFocus />;
}

/** The `agent_panel` slot: the shown project's agent panel (nothing before a project is added). */
export function AgentPanel() {
  const { project } = useSelection();
  if (project === null) return <section className="agent-panel" />;
  return <ProjectAgentPanel key={project} project={project} />;
}
