// The Project agent panel (T-0330, T-0458; SHL-04, SHB-03, PAG-01…PAG-07, ATT-03; d0113, d0087, d0068, d0060, d0077;
// *18 · Project agents & terminals → Row · Pane: empty, hidden, tool problems, menu (0087)*, *Row · More pane states
// (0086 · V)*, *Row · Switching agents, attention (0086 · V)*, *24 → Row · The Project agent*): the `pane` slot, 420 px
// (or as dragged, T-0502) between the rail and the main area while open. It shows one conversation, or the empty panel
// "Ask the Project agent" whose first message starts one (d0087). Its header has "Project agent", the switcher (the
// conversation ▾), the state pill, …, Recently ended, +, Expand (T-0502) and the close icon; then the agent line,
// Created and changed, its real terminal (T-0216's component, `terminal.attach`) and the footer. The close icon, ⌘L
// and Esc (outside the terminal) fold it back into the Ask bar. Everything it says comes from `pane.view`.
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
} from "../../components";
import type { AgentCloseDialog } from "../../generated/AgentCloseDialog";
import type { PaneAgent } from "../../generated/PaneAgent";
import type { PaneAgentLine } from "../../generated/PaneAgentLine";
import type { PaneButton } from "../../generated/PaneButton";
import type { PaneCreated } from "../../generated/PaneCreated";
import type { PaneEmpty } from "../../generated/PaneEmpty";
import type { PaneEnded } from "../../generated/PaneEnded";
import type { PaneParams } from "../../generated/PaneParams";
import type { PaneSwitcher } from "../../generated/PaneSwitcher";
import type { PaneView } from "../../generated/PaneView";
import type { RecentAgents } from "../../generated/RecentAgents";
import type { ToolProblemCallout as ToolProblemCalloutView } from "../../generated/ToolProblemCallout";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Terminal, type TerminalHandle } from "../../terminal/Terminal";
import { Icon } from "../../theme/Icon";
import { showAgentOnBoard } from "../board/filter";
import { toolProblemAction } from "../tool_problems/toolProblemRunner";
import { lookedAt } from "../ask_bar/logic";
import {
  chosenAgent,
  closePane,
  inPanel,
  focusFits,
  focusTarget,
  hasChoice,
  leaveExpand,
  newProjectAgent,
  type PaneFocusRequest,
  paneFocusRequests,
  sendFirstMessage,
  showAgent,
  shownAgents,
  startOn,
  toggleExpandPane,
} from "./logic";
import { CloseAgentDialog, RecentList } from "./parts";

/** The service events that change what the pane shows. */
export const PANE_EVENTS = [
  "project_agent_changed",
  "ticket_changed",
  "ticket_deleted",
  "projects_changed",
  // The user's keys in its tooltips (KEY-08).
  "settings_changed",
] as const;

const ignore = () => undefined;

/**
 * The newest focus request the live pane carried out. Kept outside the component: a request from another project's
 * strip item lands in the pane mounted for that project.
 */
const focusHandled: { last: PaneFocusRequest | null } = { last: null };

/** A header button with its tooltip (and keys). */
function HeaderButton({
  button,
  open,
  onClick,
  ref,
}: {
  button: PaneButton;
  open?: boolean;
  onClick: () => void;
  ref?: React.Ref<HTMLButtonElement>;
}) {
  return (
    <Tooltip text={button.label} keys={button.keys ?? undefined}>
      <button
        ref={ref}
        type="button"
        className="pane-icon-button"
        aria-label={button.label}
        aria-expanded={open}
        data-open={open || undefined}
        onClick={onClick}
      >
        <Icon name={button.icon} size={15} />
      </button>
    </Tooltip>
  );
}

/** The line under the header: the tool avatar (hidden with one tool), the named agent, the tokens or "starts when you send". */
function AgentLine({ line, onRename }: { line: PaneAgentLine; onRename?: () => void }) {
  return (
    <div className="pane-line">
      {line.avatar !== null && <ToolAvatar avatar={line.avatar} />}
      <span className="pane-line-tool">{line.agent}</span>
      <span className="pane-line-note" data-tokens={line.rename !== null || undefined}>
        {line.note}
      </span>
      <span className="pane-spacer" />
      {line.rename !== null && (
        <button type="button" className="pane-rename" onClick={onRename}>
          <Icon name="pencil" size={11} />
          <span>{line.rename}</span>
        </button>
      )}
    </div>
  );
}

/** A tool problem's notice under the agent line (M23, SHL-04): amber for a tool that isn't available, neutral for a
 * limit, with Sign in…, Resume after reset or "New agent on Sonnet". */
function Notice({ notice, project }: { notice: ToolProblemCalloutView; project: string }) {
  return (
    <div className="pane-notice">
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

/** The switcher's list (`h038v`): the project's agents with their states, then the empty pane. */
export function SwitcherList({
  switcher,
  onPick,
}: {
  switcher: PaneSwitcher;
  onPick: (agent: string | null) => void;
}) {
  return (
    <div className="pane-switcher" role="menu">
      {switcher.rows.map((row) => (
        <button
          key={row.id}
          type="button"
          role="menuitem"
          className="pane-switcher-row"
          data-selected={row.selected || undefined}
          data-palette={row.pill.palette}
          onClick={() => {
            onPick(row.id);
          }}
        >
          <Icon name="bot" size={14} />
          <span className="pane-switcher-name" data-unnamed={row.unnamed || undefined}>
            {row.name}
          </span>
          {row.avatar !== null && <ToolAvatar avatar={row.avatar} />}
          <span className="pane-switcher-state">{row.pill.label}</span>
        </button>
      ))}
      <button
        type="button"
        role="menuitem"
        className="pane-switcher-row pane-switcher-new"
        onClick={() => {
          onPick(null);
        }}
      >
        <Icon name="plus" size={14} />
        <span className="pane-switcher-name">{switcher.new_label}</span>
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
      className="pane-name-input"
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

/** Created and changed (collapsible; folded to one line at the 1280 class): links, Delete, Show on board. */
function CreatedBox({
  created,
  open,
  onToggle,
  onShowOnBoard,
}: {
  created: PaneCreated;
  open: boolean;
  onToggle: () => void;
  onShowOnBoard: () => void;
}) {
  return (
    <div className="pane-created" data-open={open || undefined}>
      <div className="pane-created-head">
        <button type="button" className="pane-created-toggle" aria-expanded={open} onClick={onToggle}>
          <Icon name={open ? "chevron-down" : "chevron-right"} size={12} />
          <span>{created.line}</span>
        </button>
        <span className="pane-spacer" />
        <button type="button" className="pane-link" onClick={onShowOnBoard}>
          {created.show_on_board.label}
        </button>
      </div>
      {open &&
        created.rows.map((row) => (
          <div
            key={`${row.key}-${String(row.edited)}`}
            className="pane-created-row"
            data-edited={row.edited || undefined}
          >
            <Icon name={row.icon} size={13} />
            <button
              type="button"
              className="pane-created-key"
              onClick={() => {
                openTicket(row.key);
              }}
            >
              {row.key}
            </button>
            <span className="pane-created-title">{row.title}</span>
            <span className="pane-created-place">{row.place}</span>
            {row.delete !== null && (
              <Tooltip text={row.delete}>
                <button
                  type="button"
                  className="pane-created-delete"
                  aria-label={`${row.delete} ${row.key}`}
                  onClick={() => {
                    openOverlay("delete_ticket", { ticket: row.key });
                  }}
                >
                  <Icon name="trash-2" size={13} />
                </button>
              </Tooltip>
            )}
          </div>
        ))}
    </div>
  );
}

/** The empty pane (`SbFln`): what it will start, the examples, Recently ended and the first-message box. */
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
  empty: PaneEmpty;
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
      <div className="pane-empty">
        <div className="pane-hello">
          <h2 className="pane-hello-title">{empty.title}</h2>
          <p className="pane-hello-text">{empty.text}</p>
        </div>
        <div className="pane-examples">
          {empty.examples.map((example) => (
            <button
              key={example.label}
              type="button"
              className="pane-example"
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
          <div className="pane-ended-list">
            <span className="pane-caps">{empty.recent_title}</span>
            {recent.rows.map((row) => (
              <div key={row.id} className="pane-ended-row">
                <Icon name="bot" size={13} />
                <span className="pane-ended-name">{row.name}</span>
                <span className="pane-ended-when">{row.ended}</span>
                <span className="pane-spacer" />
                <button
                  type="button"
                  className="pane-link"
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
      <div className="pane-composer">
        <div className="pane-input">
          <textarea
            {...PLAIN_TEXT}
            ref={box}
            className="pane-box"
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
                if (!leaveExpand()) closePane();
                return;
              }
              if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
              e.preventDefault();
              onSend();
            }}
          />
          <div className="pane-input-row">
            <span className="pane-input-hint">{empty.hint}</span>
            <span className="pane-spacer" />
            <button
              type="button"
              className="pane-send"
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
  agent: PaneAgent;
  project: string;
  name: string;
  createdOpen: boolean;
  onToggleCreated: () => void;
  onShowOnBoard: () => void;
  onRename: () => void;
  onResume: () => void;
  terminal: React.Ref<TerminalHandle>;
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
        />
      )}
      <div className="pane-well">
        <div className="pane-terminal">
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
      <div className="pane-footer" data-ended={agent.ended || undefined}>
        <Icon name={agent.ended ? "circle-stop" : "keyboard"} size={12} />
        <span className="pane-footer-text">{footer.text}</span>
        <span className="pane-spacer" />
        {footer.keys !== null && <Kbd keys={footer.keys} />}
        <span className="pane-footer-label">{footer.keys_label}</span>
      </div>
      {footer.resume !== null && (
        <div className="pane-resume">
          <Button
            compact
            variant={footer.resume.primary ? "primary" : "secondary"}
            icon="play"
            label={footer.resume.label}
            onClick={onResume}
          />
          {footer.resume_note !== null && <span className="pane-resume-note">{footer.resume_note}</span>}
        </div>
      )}
    </>
  );
}

type Floating = "switcher" | "menu" | "recent" | null;

export interface PaneBodyProps {
  view: PaneView;
  project: string;
  /** Follows ⌘L, ⇧⌘L and the strip's focus requests (the live pane; not the gallery's copies). */
  followFocus?: boolean;
}

/** The pane for a loaded view (also what the gallery and the tests render). */
export function PaneBody({ view, project, followFocus = false }: PaneBodyProps) {
  const { header, running } = view;
  const expanded = useStore(shellLayout).agentExpanded;
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const [floating, setFloating] = useState<Floating>(null);
  const [dialog, setDialog] = useState<AgentCloseDialog | null>(null);
  // The user's own fold of Created and changed, per agent; else the view's (folded at the 1280 class).
  const [createdOpen, setCreatedOpen] = useState<Readonly<Record<string, boolean>>>({});
  const nameButton = useRef<HTMLButtonElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const recentButton = useRef<HTMLButtonElement>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const terminal = useRef<TerminalHandle>(null);
  const asked = useStore(paneFocusRequests);
  const target = focusTarget(view);

  // ⌘L, ⇧⌘L, Send and Needs you ask for the keyboard: once per request, when the pane shows what it was asked for.
  useEffect(() => {
    if (!followFocus || !asked || asked === focusHandled.last || !focusFits(asked, view.agent)) return;
    if (target === "none") return;
    focusHandled.last = asked;
    if (target === "terminal") terminal.current?.focus();
    else box.current?.focus();
  }, [followFocus, asked, view.agent, target]);

  const close = () => {
    setFloating(null);
  };
  const resume = (id: string) => {
    close();
    request("pane.resume", { id }).then((shown) => {
      showAgent(project, shown as string);
    }, ignore);
  };
  const rename = (name: string) => {
    if (view.agent !== null) request("pane.rename", { id: view.agent, name }).catch(ignore);
  };
  const end = (confirm: boolean) => {
    if (view.agent === null) return;
    request("pane.end", { project, id: view.agent, confirm }).then((ended) => {
      const outcome = ended as PaneEnded;
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
        request("pane.close_ended", { project }).catch(ignore);
        break;
      case "recent":
        setFloating("recent");
        break;
      // The menu's ids are `agent_pane.rs`'s; another one does nothing here.
      default:
        break;
    }
  };
  const created = running?.created ?? null;
  const open = view.agent !== null && created !== null ? (createdOpen[view.agent] ?? !created.folded) : false;

  let title: ReactNode = (
    <span
      className="pane-title"
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
      <button
        ref={nameButton}
        type="button"
        className="pane-switch"
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
    );
  }

  return (
    <section className="pane" aria-label={header.label} data-agent={view.agent ?? undefined}>
      <div className="pane-header">
        <span className="pane-bot" data-running={running !== null || undefined}>
          <Icon name={header.icon} size={15} />
        </span>
        <span className="pane-label">{header.label}</span>
        {title}
        {header.pill !== null && <StatusPill pill={header.pill} />}
        <span className="pane-spacer" />
        <HeaderButton
          ref={moreButton}
          button={header.more}
          open={floating === "menu"}
          onClick={() => {
            setFloating(floating === "menu" ? null : "menu");
          }}
        />
        <HeaderButton
          ref={recentButton}
          button={header.recent}
          open={floating === "recent"}
          onClick={() => {
            setFloating(floating === "recent" ? null : "recent");
          }}
        />
        {running !== null && (
          <HeaderButton
            button={header.new}
            onClick={() => {
              newProjectAgent();
            }}
          />
        )}
        <HeaderButton
          button={expanded ? header.collapse : header.expand}
          onClick={() => {
            toggleExpandPane();
          }}
        />
        <HeaderButton
          button={header.close}
          onClick={() => {
            closePane();
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
          />
        </Popover>
      )}
      {floating === "menu" && (
        <AnchoredMenu
          anchor={moreButton}
          align="end"
          label={header.more.label}
          items={view.menu}
          width={260}
          onChoose={choose}
          onClose={close}
        />
      )}
      {floating === "recent" && (
        <Popover label={header.recent.label} anchor={recentButton} align="end" onClose={close}>
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
  closePane();
  return true;
}

/** Esc while expanded (`leaveExpand`), as a shortcut handler. */
function leaveExpandKey(): boolean {
  return leaveExpand();
}

/** One project's pane: asks `pane.view` for the agent the page shows, and opens on `opens_on` the first time. */
function ProjectPane({ project }: { project: string }) {
  const shown = useStore(shownAgents);
  const starts = useStore(startOn);
  const chosen = chosenAgent(shown, project);
  // After "New agent on …": the empty pane names that agent and its first message starts it.
  const startsOn = chosen === null ? starts[project] : undefined;
  const chose = hasChoice(shown, project);
  const width = useWindowWidth();
  const cls = useSizeClass();
  // Created and changed folds by the window's size class: ask again when the class changes, not on every pixel.
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the width is read when the class changes
  const windowWidth = useMemo(() => width, [cls]);
  const load = useCallback(() => {
    const params: PaneParams = {
      project,
      agent: chosen,
      window_width: windowWidth,
      ...(startsOn !== undefined && { start_on: startsOn }),
    };
    return request("pane.view", params) as Promise<PaneView>;
  }, [project, chosen, windowWidth, startsOn]);
  const { view } = useServiceView(load, PANE_EVENTS);
  useEffect(() => {
    if (!chose && view?.opens_on != null) showAgent(project, view.opens_on);
  }, [chose, view?.opens_on, project]);
  useShortcut("close_panel", foldFromInside);
  // Added after the fold, so tried before it: Esc leaves Expand first.
  useShortcut("close_panel", leaveExpandKey);
  // The user looks at the shown conversation while the panel is open, until it closes or shows another: its replies
  // until then aren't new on the Ask bar (SHB-03).
  const open = useStore(shellLayout).agentOpen;
  const shownAgent = view?.agent ?? null;
  useEffect(() => {
    if (!open || shownAgent === null) return;
    lookedAt(shownAgent);
    return () => {
      lookedAt(shownAgent);
    };
  }, [open, shownAgent]);
  if (!view) return <section className="pane" />;
  return <PaneBody view={view} project={project} followFocus />;
}

/** The `pane` slot: the shown project's pane (nothing before a project is added). */
export function Pane() {
  const { project } = useSelection();
  if (project === null) return <section className="pane" />;
  return <ProjectPane key={project} project={project} />;
}
