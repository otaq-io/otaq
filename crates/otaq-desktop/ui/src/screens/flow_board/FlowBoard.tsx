// Edit flow on the board: the board itself becomes the flow's editor, in place. The toolbar gives way to the editor's
// bar ("Editing shop-web's flow", the changes with Undo, Watch a ticket, Start from…, ⋯, Cancel, Save flow); each
// column header becomes its stage (name, what happens, agent, skill, and for a check its rounds and where fails go),
// the board's cards stay in their columns, dimmed, and Backlog and Done are locked at the ends (Done carries how work
// leaves the flow). A stage's header is dragged (or moved with Alt+←/→) to reorder, × removes it, **+** between two
// columns opens the palette of ready stages, and a click opens its details beside the columns. The first time, a
// one-time hint says how it works. Everything it says is `flow_board.view`'s (`otaq-view`'s `flow_board.rs`); every
// edit goes to the project's draft, and nothing changes on the board until Save flow.
import { type PointerEvent, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { useShortcut } from "../../app/registry";
import { useStore } from "../../app/store";
import { TicketCard } from "../../cards";
import {
  AnchoredMenu,
  Button,
  Checkbox,
  CodeText,
  IconButton,
  OneTimeHint,
  Popover,
  Radio,
  Segmented,
  Select,
  TextArea,
  TextInput,
  ToolAvatar,
  Tooltip,
} from "../../components";
import type { BoardView } from "../../generated/BoardView";
import type { FlowBoardColumn } from "../../generated/FlowBoardColumn";
import type { FlowBoardDelivery } from "../../generated/FlowBoardDelivery";
import type { FlowBoardDeliverySkill } from "../../generated/FlowBoardDeliverySkill";
import type { FlowBoardDetails } from "../../generated/FlowBoardDetails";
import type { FlowBoardParams } from "../../generated/FlowBoardParams";
import type { FlowBoardView } from "../../generated/FlowBoardView";
import type { FlowEditorChoices } from "../../generated/FlowEditorChoices";
import type { FlowEditorPick } from "../../generated/FlowEditorPick";
import type { FlowEditorStageEdit } from "../../generated/FlowEditorStageEdit";
import type { MenuItem } from "../../generated/MenuItem";
import type { NewToast } from "../../generated/NewToast";
import { asRequestError, useServiceView } from "../../service";
import { pickFlowFile, request, saveFlowFile } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { cardsOf } from "../board/Column";
import { showAgentInPanel } from "../agent_panel/logic";
import { type EditorActions, FixLine, SkillField } from "../flow_editor/FlowEditor";
import {
  editorAgents,
  editStage,
  NO_CHOICES,
  pickEntry,
  skillButton,
  writeSkill,
} from "../flow_editor/logic";
import {
  askForChanges,
  clampStep,
  closeFlowBoard,
  discardDraft,
  dropDraft,
  dropTo,
  exportFile,
  type Box,
  FLOW_BOARD_EVENTS,
  type FlowBoardOpen,
  hintSpot,
  importFile,
  keyMove,
  restore,
  saveDraft,
  stepOf,
  suggest,
} from "./logic";
import "./flow_board.css";

const ignore = () => undefined;

/** A refused request as a toast: the service's own words. */
function toast(error: unknown) {
  const notice: NewToast = {
    level: "error",
    title: asRequestError(error).message,
    text: null,
    link: null,
    ticket: null,
  };
  request("notices.toast", notice).catch(ignore);
}

/** The palette's place: the gap's title, what it adds after, and the + it opens from. */
interface PaletteAt {
  after: string | null;
  title: string;
  anchor: HTMLElement;
}

/** The pretend ticket's walk. */
interface Watching {
  fails: boolean;
  at: number;
}

export interface FlowBoardProps {
  open: FlowBoardOpen;
  /** The board as it is, for its cards (dimmed under the headers). */
  board: BoardView | undefined;
}

export function FlowBoard({ open, board }: FlowBoardProps) {
  const project = open.project;
  const [selected, setSelected] = useState<string | null>(open.select ?? null);
  const [focusSkill, setFocusSkill] = useState<string | null>(
    open.focusSkill === true ? (open.select ?? null) : null,
  );
  const [choices, setChoices] = useState<FlowEditorChoices>(NO_CHOICES);
  const [history, setHistory] = useState<string[]>([]);
  const [palette, setPalette] = useState<PaletteAt | null>(null);
  const [startFrom, setStartFrom] = useState(false);
  const [more, setMore] = useState(false);
  const [watching, setWatching] = useState<Watching | null>(null);
  const [asking, setAsking] = useState(false);
  const [writing, setWriting] = useState<{ stage: string; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const agents = useStore(editorAgents);
  const agent = agents[project] ?? null;
  const bar = useRef<HTMLDivElement>(null);
  const startButton = useRef<HTMLButtonElement>(null);
  // The first-time hint sits under Watch a ticket, which it names, so it never covers Backlog's column.
  const watchButton = useRef<HTMLButtonElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const askButton = useRef<HTMLButtonElement>(null);

  const load = useCallback(() => {
    const p: FlowBoardParams = {
      project,
      ...(selected !== null && { selected }),
      ...(agent !== null && { agent }),
      choices,
    };
    return request("flow_board.view", p) as Promise<FlowBoardView>;
  }, [project, selected, agent, choices]);
  const state = useServiceView(load, FLOW_BOARD_EVENTS);
  const view = state.view;

  // Every edit keeps the document before it, so Undo can put it back.
  const edit = useCallback(
    (e: FlowEditorStageEdit) => {
      if (view !== undefined) setHistory((h) => [...h, view.yaml]);
      editStage(project, e).then((id) => {
        if (id !== null) setSelected(id);
      }, ignore);
    },
    [project, view],
  );
  const undo = useCallback(() => {
    const last = history.at(-1);
    if (last === undefined) return;
    setHistory((h) => h.slice(0, -1));
    restore(project, last).catch(toast);
  }, [history, project]);
  const pick = useCallback(
    (p: FlowEditorPick) => {
      if (view !== undefined) setHistory((h) => [...h, view.yaml]);
      setSelected(null);
      setStartFrom(false);
      setMore(false);
      pickEntry(project, p).catch(ignore);
    },
    [project, view],
  );
  const finish = useCallback((sent: Promise<unknown>) => {
    setBusy(true);
    sent.then(
      () => {
        setBusy(false);
        closeFlowBoard();
      },
      (error: unknown) => {
        setBusy(false);
        toast(error);
      },
    );
  }, []);

  // Add a stage after <stage> (a column's ⋯): the palette opens at that gap once the columns are drawn.
  const opened = useRef(false);
  useEffect(() => {
    if (opened.current || open.addAfter === undefined || view === undefined) return;
    opened.current = true;
    const column = view.columns.find((c) =>
      open.addAfter === null ? c.kind === "backlog" : c.id === open.addAfter,
    );
    const gap = column?.add;
    const button = document.querySelector<HTMLElement>(`[data-fb-gap="${CSS.escape(column?.id ?? "")}"]`);
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the + it opens from exists only once the columns are drawn
    if (gap && button) setPalette({ after: gap.after, title: gap.title, anchor: button });
  }, [open.addAfter, view]);

  // Esc closes what is open, then the details; the editor itself stays until Cancel or Save flow. The palette and the
  // other popovers close on their own Esc. Esc is the page's Close: it comes here as that key, or as a key a focused
  // field in the details passed on.
  const closeOne = useCallback(() => {
    if (watching !== null) setWatching(null);
    else if (selected !== null) setSelected(null);
    else return false;
    return true;
  }, [selected, watching]);
  useShortcut("close_sheet", closeOne);
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (closeOne()) e.preventDefault();
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
    };
  }, [closeOne]);

  // The SkillField's clicks, as the board's editor runs them: Ask the agent to write it opens a box for the user's
  // words (nothing is sent before), the other buttons go to the draft.
  const skillActions = useMemo<EditorActions>(
    () => ({
      edit,
      write: (stage, prefill) => {
        setWriting({ stage, text: prefill });
      },
      skill: (stage, button) => {
        skillButton(project, stage, button).catch(ignore);
      },
      fix: setChoices,
    }),
    [edit, project],
  );

  // Drag a stage's header to reorder: the column under the pointer on release is where it goes.
  const drag = useRef<{ from: number; x: number; y: number; moved: boolean } | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [lifted, setLifted] = useState<number | null>(null);
  const stageAt = (x: number): number | null => {
    for (const el of document.querySelectorAll<HTMLElement>("[data-fb-index]")) {
      const r = el.getBoundingClientRect();
      if (x >= r.left && x <= r.right) return Number(el.dataset.fbIndex);
    }
    return null;
  };
  const onHeadDown = (column: FlowBoardColumn) => (e: PointerEvent<HTMLElement>) => {
    if (e.button !== 0 || column.index === null || (e.target as HTMLElement).closest("button")) return;
    drag.current = { from: column.index, x: e.clientX, y: e.clientY, moved: false };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onHeadMove = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (d === null) return;
    if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 5) return;
    d.moved = true;
    setLifted(d.from);
    setOver(stageAt(e.clientX));
  };
  const onHeadUp = (column: FlowBoardColumn) => (e: PointerEvent<HTMLElement>) => {
    const d = drag.current;
    drag.current = null;
    setLifted(null);
    setOver(null);
    if (d === null) return;
    if (!d.moved) {
      setFocusSkill(null);
      setSelected(column.id);
      return;
    }
    const onto = stageAt(e.clientX);
    const to = onto === null ? null : dropTo(d.from, onto);
    if (to !== null) edit({ edit: "move", stage: column.id, to });
  };

  // The first-time hint's place, measured once the stages are drawn and again as they change or the window resizes.
  const [spot, setSpot] = useState<Box | null>(null);
  useLayoutEffect(() => {
    const place = () => {
      const heads = [...document.querySelectorAll<HTMLElement>(".fb-head")].map((h) =>
        h.getBoundingClientRect(),
      );
      const next = hintSpot(watchButton.current?.getBoundingClientRect(), heads);
      setSpot((was) =>
        was !== null &&
        next !== null &&
        was.left === next.left &&
        was.top === next.top &&
        was.width === next.width
          ? was
          : next,
      );
    };
    place();
    window.addEventListener("resize", place);
    return () => {
      window.removeEventListener("resize", place);
    };
  }, [view, selected, watching]);

  if (view === undefined) {
    return (
      <div className="fb-screen">
        {state.error !== undefined && <FlowBoardProblem project={project} message={state.error.message} />}
      </div>
    );
  }

  const stageCount = view.columns.filter((c) => c.kind === "stage").length;
  const steps = watching === null ? [] : watching.fails ? view.watch.fails : view.watch.passes;
  const step = watching === null ? null : (steps[clampStep(watching.at, steps)] ?? null);
  const boardColumn = (id: string) => board?.columns.find((c) => c.id === id);
  const suggestion = view.suggestion;

  return (
    <div className="fb-screen">
      <div ref={bar} className="fb-bar" role="toolbar" aria-label={view.bar.title}>
        <div className="fb-bar-title">
          <Icon name="pencil" size={14} />
          <div className="fb-bar-words">
            <span className="fb-bar-name">{suggestion?.title ?? view.bar.title}</span>
            {suggestion !== null ? (
              <span className="fb-bar-line">{suggestion.line}</span>
            ) : view.bar.changes !== null ? (
              <span className="fb-bar-line">
                <span className="fb-bar-count">{view.bar.changes}</span>
                {view.bar.change_words !== null && <span>{view.bar.change_words}</span>}
                {history.length > 0 && (
                  <button type="button" className="fb-link" onClick={undo}>
                    {view.bar.undo}
                  </button>
                )}
              </span>
            ) : (
              <span className="fb-bar-line">{view.bar.line}</span>
            )}
          </div>
        </div>
        <span className="fb-spacer" />
        {suggestion !== null ? (
          <>
            <Button
              ref={askButton}
              compact
              variant="secondary"
              icon="message-square"
              label={suggestion.ask}
              onClick={() => {
                setAsking(true);
              }}
            />
            <Button
              compact
              variant="ghost"
              label={suggestion.discard}
              disabled={busy}
              onClick={() => {
                setHistory([]);
                dropDraft(view).catch(toast);
              }}
            />
          </>
        ) : (
          <>
            <Button
              ref={watchButton}
              compact
              variant="ghost"
              icon="play"
              label={view.watch.label}
              onClick={() => {
                setSelected(null);
                setWatching(watching === null ? { fails: false, at: 0 } : null);
              }}
            />
            <Button
              ref={startButton}
              compact
              variant="ghost"
              icon="layers"
              label={view.bar.start_from}
              onClick={() => {
                setStartFrom(true);
              }}
            />
            <IconButton
              ref={moreButton}
              icon="ellipsis"
              label={view.bar.more}
              onClick={() => {
                setMore(true);
              }}
            />
            <Button
              compact
              variant="secondary"
              label={view.bar.cancel}
              disabled={busy}
              onClick={() => {
                finish(view.proposal === 0 ? Promise.resolve() : dropDraft(view));
              }}
            />
          </>
        )}
        <Tooltip text={view.bar.reason ?? view.bar.applies ?? ""}>
          <Button
            compact
            variant="primary"
            label={view.bar.save}
            disabled={!view.bar.save_enabled || busy}
            loading={busy}
            onClick={() => {
              finish(saveDraft(view));
            }}
          />
        </Tooltip>
      </div>
      {view.drafting !== null && (
        <div
          className="fb-drafting"
          role="status"
          data-needs-you={view.drafting.needs_you !== null || undefined}
        >
          <Icon name={view.drafting.needs_you !== null ? "hand" : "loader-circle"} size={14} />
          {view.drafting.needs_you !== null && (
            <strong className="fb-needs-you">{view.drafting.needs_you}</strong>
          )}
          <span>{view.drafting.text}</span>
          <button
            type="button"
            className="fb-link"
            onClick={() => {
              if (view.drafting !== null) showAgentInPanel(project, view.drafting.agent);
            }}
          >
            {view.drafting.show}
          </button>
        </div>
      )}
      {view.bar.applies !== null && suggestion === null && (
        <p className="fb-applies">
          <Icon name="info" size={13} />
          <span>{view.bar.applies}</span>
        </p>
      )}
      {state.error !== undefined && (
        // The view didn't reload (a stage clicked, an edit made): what went wrong, over the editor as it last drew.
        <p className="fb-problem" role="alert">
          {state.error.message}
        </p>
      )}
      <div className="fb-board">
        <div className="fb-columns">
          {view.columns.map((column) => {
            const shown = boardColumn(column.id);
            const cards = shown ? cardsOf(shown) : [];
            const here = step !== null && step.column === column.id;
            return (
              <div key={column.id} className="fb-slot">
                <section
                  className="fb-column"
                  data-kind={column.kind}
                  data-selected={column.selected || undefined}
                  data-marked={column.mark !== null || undefined}
                  data-lifted={lifted !== null && column.index === lifted ? true : undefined}
                  data-over={over !== null && column.index === over && over !== lifted ? true : undefined}
                  data-fb-index={column.index ?? undefined}
                  aria-label={column.name}
                >
                  <ColumnHead
                    column={column}
                    count={stageCount}
                    onDown={onHeadDown(column)}
                    onMove={onHeadMove}
                    onUp={onHeadUp(column)}
                    onSelect={() => {
                      setSelected(column.id);
                    }}
                    onMoveKey={(to) => {
                      edit({ edit: "move", stage: column.id, to });
                    }}
                    onRemove={() => {
                      if (selected === column.id) setSelected(null);
                      edit({ edit: "remove", stage: column.id });
                    }}
                  />
                  <div className="fb-cards">
                    {here && (
                      <div className="fb-pretend" role="status">
                        <span className="fb-pretend-label">{view.watch.ticket_label}</span>
                        <span className="fb-pretend-title">{view.watch.ticket_title}</span>
                        <span className="fb-pretend-state">{step.state}</span>
                      </div>
                    )}
                    <div className="fb-dimmed" aria-hidden inert>
                      {cards.map((c) => (
                        <TicketCard key={c.card.key} card={c.card} />
                      ))}
                    </div>
                  </div>
                </section>
                {column.add !== null && (
                  <Tooltip text={column.add.title}>
                    <button
                      type="button"
                      className="fb-gap"
                      data-fb-gap={column.id}
                      aria-label={column.add.title}
                      onClick={(e) => {
                        const gap = column.add;
                        if (gap !== null)
                          setPalette({ after: gap.after, title: gap.title, anchor: e.currentTarget });
                      }}
                    >
                      <Icon name="plus" size={13} />
                    </button>
                  </Tooltip>
                )}
              </div>
            );
          })}
        </div>
        {watching !== null && step !== null ? (
          <aside className="fb-side fb-watch" aria-label={view.watch.title}>
            <header className="fb-side-head">
              <div>
                <h4>{view.watch.title}</h4>
                <span className="fb-muted">{view.watch.subtitle}</span>
              </div>
              <span className="fb-spacer" />
              <IconButton
                icon="x"
                label={view.watch.close}
                onClick={() => {
                  setWatching(null);
                }}
              />
            </header>
            <Segmented
              label={view.watch.title}
              options={
                view.watch.fails.length > 0
                  ? [view.watch.passes_label, view.watch.fails_label]
                  : [view.watch.passes_label]
              }
              selected={watching.fails ? 1 : 0}
              onSelect={(i) => {
                setWatching({ fails: i === 1, at: 0 });
              }}
            />
            <ol className="fb-path">
              {steps.map((s, i) => (
                <li
                  // A walk passes a stage more than once: its place in the walk tells them apart.
                  key={`${String(i)}-${s.column}`}
                  data-at={i === clampStep(watching.at, steps) || undefined}
                  data-done={i < watching.at || undefined}
                >
                  {s.name}
                </li>
              ))}
            </ol>
            <span className="fb-step-of">
              {stepOf(view.watch.step_of, clampStep(watching.at, steps), steps.length)}
            </span>
            <p className="fb-step-text">{step.text}</p>
            <div className="fb-side-buttons">
              <Button
                compact
                variant="secondary"
                label={view.watch.back}
                disabled={watching.at <= 0}
                onClick={() => {
                  setWatching({ ...watching, at: clampStep(watching.at - 1, steps) });
                }}
              />
              <Button
                compact
                variant="primary"
                label={view.watch.next}
                disabled={watching.at >= steps.length - 1}
                onClick={() => {
                  setWatching({ ...watching, at: clampStep(watching.at + 1, steps) });
                }}
              />
            </div>
          </aside>
        ) : view.details !== null ? (
          <StagePanel
            key={view.details.stage}
            details={view.details}
            focusSkill={focusSkill === view.details.stage}
            actions={skillActions}
            onEdit={edit}
            onClose={() => {
              setSelected(null);
            }}
          />
        ) : view.delivery !== null ? (
          <DeliveryPanel
            delivery={view.delivery}
            focusSkill={focusSkill === "done"}
            onEdit={edit}
            onClose={() => {
              setSelected(null);
            }}
          />
        ) : suggestion !== null && suggestion.whys.length > 0 ? (
          <aside className="fb-side fb-whys" aria-label={suggestion.whys_title}>
            <h4>{suggestion.whys_title}</h4>
            <ol>
              {suggestion.whys.map((w) => (
                <li key={w.number}>
                  <span className="fb-why-number">{w.number}</span>
                  <div>
                    <span className="fb-why-title">{w.title}</span>
                    {w.why !== null && <p className="fb-muted">{w.why}</p>}
                  </div>
                </li>
              ))}
            </ol>
          </aside>
        ) : null}
      </div>
      {palette !== null && (
        <Popover
          label={palette.title}
          anchor={{ current: palette.anchor }}
          onClose={() => {
            setPalette(null);
          }}
        >
          <div className="fb-palette">
            <header className="fb-pop-head">
              <span>{palette.title}</span>
              <button
                type="button"
                className="fb-pop-esc"
                onClick={() => {
                  setPalette(null);
                }}
              >
                esc
              </button>
            </header>
            {view.palette.templates.map((t) => (
              <button
                key={t.id}
                type="button"
                className="fb-pop-row"
                onClick={() => {
                  setPalette(null);
                  edit({
                    edit: "add_template",
                    template: t.id,
                    ...(palette.after !== null && { after: palette.after }),
                  });
                }}
              >
                <Icon name={t.icon} size={15} />
                <span className="fb-pop-words">
                  <span className="fb-pop-name">{t.title}</span>
                  <span className="fb-muted">{t.line}</span>
                  {t.warning !== null && (
                    <span className="fb-head-warning fb-pop-warning">
                      <Icon name="triangle-alert" size={11} />
                      <span>
                        <CodeText text={t.warning} />
                      </span>
                    </span>
                  )}
                </span>
              </button>
            ))}
            <p className="fb-pop-foot">{view.palette.foot}</p>
          </div>
        </Popover>
      )}
      {startFrom && (
        <Popover
          label={view.bar.start_from}
          anchor={startButton}
          align="end"
          onClose={() => {
            setStartFrom(false);
          }}
        >
          <div className="fb-start">
            <button
              type="button"
              className="fb-pop-row fb-suggest"
              disabled={view.start_from.suggest.off !== null}
              onClick={() => {
                setStartFrom(false);
                suggest(project).then((id) => {
                  showAgentInPanel(project, id);
                }, toast);
              }}
            >
              <Icon name="sparkles" size={15} />
              <span className="fb-pop-words">
                <span className="fb-pop-name">{view.start_from.suggest.label}</span>
                <span className="fb-muted">
                  {view.start_from.suggest.off ?? view.start_from.suggest.text}
                </span>
              </span>
            </button>
            <span className="fb-pop-section">{view.start_from.shipped_label}</span>
            {view.start_from.presets.map((p) => (
              <button
                key={p.key}
                type="button"
                className="fb-pop-row"
                onClick={() => {
                  pick(p.pick);
                }}
              >
                <span className="fb-pop-words">
                  <span className="fb-pop-name">{p.name}</span>
                  <span className="fb-muted">{p.line}</span>
                </span>
              </button>
            ))}
            <span className="fb-pop-section">{view.start_from.more_label}</span>
            {view.start_from.others.map((p) => (
              <button
                key={p.key}
                type="button"
                className="fb-pop-row"
                onClick={() => {
                  pick(p.pick);
                }}
              >
                <span className="fb-pop-words">
                  <span className="fb-pop-name">{p.name}</span>
                  <span className="fb-muted">{p.line}</span>
                </span>
              </button>
            ))}
            <button
              type="button"
              className="fb-pop-row"
              onClick={() => {
                setStartFrom(false);
                setHistory((h) => [...h, view.yaml]);
                importFile(project, () => pickFlowFile(view.more.import_title)).catch(toast);
              }}
            >
              <Icon name="folder-open" size={15} />
              <span className="fb-pop-words">
                <span className="fb-pop-name">{view.start_from.import}</span>
                <span className="fb-muted">{view.start_from.import_line}</span>
              </span>
            </button>
            <p className="fb-pop-foot">{view.start_from.foot}</p>
          </div>
        </Popover>
      )}
      {more && (
        <AnchoredMenu
          anchor={moreButton}
          align="end"
          label={view.bar.more}
          items={moreItems(view)}
          onChoose={(id) => {
            setMore(false);
            moreChoice(view, id, setHistory);
          }}
          onClose={() => {
            setMore(false);
          }}
        />
      )}
      {asking && suggestion !== null && (
        <AskBox
          anchor={askButton}
          label={suggestion.ask}
          placeholder={suggestion.ask_placeholder}
          initial=""
          onSend={(text) => {
            setAsking(false);
            askForChanges(view, text).catch(toast);
          }}
          onClose={() => {
            setAsking(false);
          }}
        />
      )}
      {writing !== null && (
        <AskBox
          anchor={bar}
          label={writing.text}
          placeholder={writing.text}
          initial={writing.text}
          onSend={(text) => {
            const stage = writing.stage;
            setWriting(null);
            writeSkill(project, stage, text).then((id) => {
              if (id !== null) showAgentInPanel(project, id);
            }, toast);
          }}
          onClose={() => {
            setWriting(null);
          }}
        />
      )}
      {spot !== null && <OneTimeHint hint={view.hint} anchor={spot} align="end" />}
    </div>
  );
}

/** ⋯ in the bar: Export, Import, Copy from another project (each project), Go back to the previous flow. */
/**
 * The editor when its view doesn't come: what went wrong, and the ways out, so the editor never stays blank. Discard
 * the draft drops whatever draft the project has open (the flow in use draws next); Close leaves the editor.
 */
export function FlowBoardProblem({ project, message }: { project: string; message: string }) {
  return (
    <div className="fb-problem-box" role="alert">
      <p className="fb-problem-title">The flow editor can’t draw this flow</p>
      <p className="fb-problem">{message}</p>
      <div className="fb-problem-actions">
        <Button
          compact
          variant="secondary"
          label="Discard the draft"
          onClick={() => {
            discardDraft(project).catch(toast);
          }}
        />
        <Button
          compact
          variant="ghost"
          label="Close"
          onClick={() => {
            closeFlowBoard();
          }}
        />
      </div>
    </div>
  );
}

function moreItems(view: FlowBoardView): MenuItem[] {
  const item = (id: string, label: string, hint: string | null, separated = false): MenuItem => ({
    id,
    label,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint,
    separated,
    checked: false,
    header: false,
  });
  const items = [
    item("export", view.more.export, view.more.export_line),
    item("import", view.more.import, view.more.import_line),
  ];
  if (view.start_from.others.length > 0) {
    items.push({ ...item("copy", view.more.copy, null, true), header: true });
    for (const other of view.start_from.others) items.push(item(other.key, other.name, other.line));
  }
  if (view.more.previous !== null)
    items.push(item("previous", view.more.previous, view.more.previous_line, true));
  return items;
}

function moreChoice(
  view: FlowBoardView,
  id: string,
  setHistory: (f: (h: string[]) => string[]) => void,
): void {
  const keep = () => {
    setHistory((h) => [...h, view.yaml]);
  };
  switch (id) {
    case "export":
      exportFile(view, saveFlowFile).catch(toast);
      return;
    case "import":
      keep();
      importFile(view.project, () => pickFlowFile(view.more.import_title)).catch(toast);
      return;
    case "previous":
      keep();
      request("settings.flow_propose", { project: view.project, source: { from: "previous" } }).catch(toast);
      return;
    default: {
      const other = view.start_from.others.find((o) => o.key === id);
      if (other !== undefined) {
        keep();
        pickEntry(view.project, other.pick).catch(ignore);
      }
    }
  }
}

function ColumnHead({
  column,
  count,
  onDown,
  onMove,
  onUp,
  onSelect,
  onMoveKey,
  onRemove,
}: {
  column: FlowBoardColumn;
  count: number;
  onDown: (e: PointerEvent<HTMLElement>) => void;
  onMove: (e: PointerEvent<HTMLElement>) => void;
  onUp: (e: PointerEvent<HTMLElement>) => void;
  onSelect: () => void;
  onMoveKey: (to: number) => void;
  onRemove: () => void;
}) {
  const fixed = column.kind !== "stage";
  const head = (
    // A stage's header is a drag handle and a button: a press selects it, a drag moves it, Alt+←/→ too.
    <div
      className="fb-head"
      role="button"
      tabIndex={0}
      aria-pressed={column.selected}
      data-fixed={fixed || undefined}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onClick={(e) => {
        // A fixed end has no drag: its click selects (Done's How work leaves the flow; Backlog has nothing).
        if (fixed && column.kind === "done") onSelect();
        e.stopPropagation();
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          if (column.kind !== "backlog") onSelect();
          return;
        }
        if (e.altKey && column.index !== null) {
          const to = keyMove(column.index, count, e.key);
          if (to !== null) {
            e.preventDefault();
            onMoveKey(to);
          }
        }
      }}
    >
      <div className="fb-head-top">
        {fixed ? <Icon name="lock" size={12} /> : <Icon name={column.icon} size={14} />}
        <span className="fb-head-name">{column.name}</span>
        {column.mark !== null && (
          <Tooltip text={column.mark.why}>
            <span className="fb-mark">{column.mark.label}</span>
          </Tooltip>
        )}
        <span className="fb-spacer" />
        {column.movable && (
          <button
            type="button"
            className="fb-remove"
            aria-label={`Remove ${column.name}`}
            onClick={(e) => {
              e.stopPropagation();
              onRemove();
            }}
          >
            <Icon name="x" size={12} />
          </button>
        )}
      </div>
      <span className="fb-head-what">{column.what}</span>
      {column.who !== null && (
        <span className="fb-head-who">
          {column.avatar !== null && <ToolAvatar avatar={column.avatar} />}
          <span>{column.who}</span>
        </span>
      )}
      {column.skill !== null && <span className="fb-head-skill">{column.skill}</span>}
      {column.check_line !== null && <span className="fb-head-line">{column.check_line}</span>}
      {column.delivery !== null && <span className="fb-head-delivery">{column.delivery}</span>}
      {column.problem !== null && <span className="fb-head-problem">{column.problem}</span>}
      {column.warning !== null && (
        <span className="fb-head-warning">
          <Icon name="triangle-alert" size={11} />
          <span>
            <CodeText text={column.warning} />
          </span>
        </span>
      )}
    </div>
  );
  return column.description === null ? head : <Tooltip text={column.description}>{head}</Tooltip>;
}

function StagePanel({
  details,
  focusSkill,
  actions,
  onEdit,
  onClose,
}: {
  details: FlowBoardDetails;
  focusSkill: boolean;
  actions: EditorActions;
  onEdit: (edit: FlowEditorStageEdit) => void;
  onClose: () => void;
}) {
  const stage = details.stage;
  const kindIndex = details.kinds.findIndex((k) => k.kind === details.kind);
  return (
    <aside className="fb-side fe-details" aria-label={details.title}>
      <header className="fb-side-head">
        <h4>{details.title}</h4>
        {details.mark !== null && (
          <Tooltip text={details.mark.why}>
            <span className="fb-mark">{details.mark.label}</span>
          </Tooltip>
        )}
        <span className="fb-spacer" />
        <IconButton icon="x" label="Close" onClick={onClose} />
      </header>
      <TextField
        key={`name-${stage}-${details.name}`}
        label={details.name_label}
        value={details.name}
        onSave={(name) => {
          if (name.trim() !== "") onEdit({ edit: "rename", stage, name });
        }}
      />
      <div className="fe-field">
        <span className="fe-label">{details.kind_label}</span>
        <Segmented
          label={details.kind_label}
          options={details.kinds.map((k) => k.label)}
          selected={kindIndex}
          onSelect={(i) => {
            const kind = details.kinds[i]?.kind;
            if (kind !== undefined && i !== kindIndex) onEdit({ edit: "kind", stage, kind });
          }}
        />
      </div>
      <TextField
        key={`description-${stage}-${details.description}`}
        label={details.description_label}
        value={details.description}
        hint={details.description_hint}
        multiline
        onSave={(description) => {
          onEdit({ edit: "description", stage, description });
        }}
      />
      {details.agent !== null && (
        <div className="fe-field">
          <span className="fe-label">{details.agent.label}</span>
          <div className="fe-agent-row">
            {details.agent.avatar !== null && <ToolAvatar avatar={details.agent.avatar} />}
            <Select
              label={details.agent.label}
              value={details.agent.name}
              items={details.agent.items}
              onChange={(agent) => {
                if (agent !== details.agent?.name) onEdit({ edit: "agent", stage, agent });
              }}
            />
          </div>
          {details.agent.warning !== null && (
            <div className="fe-fix" data-tone="amber">
              <Icon name="triangle-alert" size={14} />
              <span className="fe-fix-text">
                <span>
                  <CodeText text={details.agent.warning} />
                </span>
              </span>
            </div>
          )}
          <span className="fe-hint">{details.agent.hint}</span>
        </div>
      )}
      {details.skill !== null && (
        <>
          <SkillField field={details.skill} stage={stage} actions={actions} focus={focusSkill} />
          <span className="fe-hint">
            {details.skill_tag !== null && <span className="fb-mark">{details.skill_tag}</span>}
            {details.skill_hint}
          </span>
        </>
      )}
      {details.fail_to !== null && (
        <div className="fe-field">
          <span className="fe-label">{details.fail_to.label}</span>
          <Select
            label={details.fail_to.label}
            value={details.fail_to.value}
            items={details.fail_to.items}
            onChange={(id) => {
              const field = details.fail_to;
              if (id === "" || field === null) return;
              onEdit(id === field.nearest ? { edit: "fail_to", stage } : { edit: "fail_to", stage, to: id });
            }}
          />
          {details.fail_to.problem !== null && (
            <p className="fe-problem" role="alert">
              {details.fail_to.problem}
            </p>
          )}
        </div>
      )}
      {details.rounds !== null && (
        <div className="fe-field">
          <span className="fe-label">{details.rounds.label}</span>
          <div className="fb-rounds">
            <IconButton
              icon="minus"
              label="One round fewer"
              disabled={details.rounds.value <= details.rounds.min}
              onClick={() => {
                if (details.rounds !== null)
                  onEdit({ edit: "rounds", stage, rounds: details.rounds.value - 1 });
              }}
            />
            <span className="fb-rounds-value">{details.rounds.value}</span>
            <IconButton
              icon="plus"
              label="One round more"
              disabled={details.rounds.value >= details.rounds.max}
              onClick={() => {
                if (details.rounds !== null)
                  onEdit({ edit: "rounds", stage, rounds: details.rounds.value + 1 });
              }}
            />
            <span className="fe-muted">{details.rounds.note}</span>
          </div>
        </div>
      )}
      <div className="fe-field">
        <Checkbox
          checked={details.skip.on}
          label={
            <span>
              {details.skip.label} <span className="fe-muted">{details.skip.note}</span>
            </span>
          }
          onChange={(on) => {
            onEdit({ edit: "option", stage, option: "skip", on });
          }}
        />
      </div>
      {details.fixes.map((fix) => (
        <FixLine key={fix.text} fix={fix} actions={actions} />
      ))}
      {details.problem !== null && (
        <p className="fe-problem" role="alert">
          {details.problem}
        </p>
      )}
      <div className="fb-remove-row">
        <Button
          compact
          variant="ghost"
          icon="trash-2"
          label={details.remove}
          onClick={() => {
            onClose();
            onEdit({ edit: "remove", stage });
          }}
        />
        {details.remove_note !== null && <span className="fe-muted">{details.remove_note}</span>}
      </div>
    </aside>
  );
}

/** A text field that saves when it loses focus, if its text changed; a new value from Rust starts it again (its key). */
function TextField({
  label,
  value,
  hint,
  multiline = false,
  onSave,
}: {
  label: string;
  value: string;
  hint?: string;
  multiline?: boolean;
  onSave: (text: string) => void;
}) {
  const [text, setText] = useState(value);
  const save = () => {
    if (text !== value) onSave(text);
  };
  return (
    <label className="fe-field">
      <span className="fe-label">{label}</span>
      {multiline ? (
        <TextArea value={text} rows={3} grow label={label} onChange={setText} onBlur={save} />
      ) : (
        <TextInput value={text} label={label} onChange={setText} onBlur={save} />
      )}
      {hint !== undefined && <span className="fe-hint">{hint}</span>}
    </label>
  );
}

function DeliveryPanel({
  delivery,
  focusSkill,
  onEdit,
  onClose,
}: {
  delivery: FlowBoardDelivery;
  /** Opened from "Set the delivery's PR skill": the PR skill field gets the keyboard. */
  focusSkill: boolean;
  onEdit: (edit: FlowEditorStageEdit) => void;
  onClose: () => void;
}) {
  const skill = delivery.skill;
  return (
    <aside className="fb-side" aria-label={delivery.title}>
      <header className="fb-side-head">
        <h4>{delivery.title}</h4>
        <span className="fb-spacer" />
        <IconButton icon="x" label="Close" onClick={onClose} />
      </header>
      <div className="fe-field" role="radiogroup" aria-label={delivery.delivers_label}>
        <span className="fe-label">{delivery.delivers_label}</span>
        {delivery.delivers.map((c) => (
          <Radio
            key={c.id}
            name="fb-delivers"
            checked={c.selected}
            label={c.label}
            onChange={(on) => {
              if (on && !c.selected)
                onEdit({ edit: "delivery", field: delivery.delivers_field, value: c.id });
            }}
          />
        ))}
        {delivery.delivers_note !== null && <span className="fe-hint">{delivery.delivers_note}</span>}
      </div>
      <div className="fe-field" role="radiogroup" aria-label={delivery.mode_label}>
        <span className="fe-label">{delivery.mode_label}</span>
        {delivery.modes.map((c) => (
          <Radio
            key={c.id}
            name="fb-mode"
            checked={c.selected}
            label={c.label}
            disabled={delivery.modes_off}
            onChange={(on) => {
              if (on && !c.selected) onEdit({ edit: "delivery", field: delivery.mode_field, value: c.id });
            }}
          />
        ))}
        {delivery.mode_note !== null && <span className="fe-hint">{delivery.mode_note}</span>}
      </div>
      <div className="fe-field">
        <span className="fe-label">{delivery.agent.label}</span>
        <Select
          label={delivery.agent.label}
          value={delivery.agent.value}
          items={delivery.agent.items}
          onChange={(agent) => {
            onEdit({ edit: "delivery", field: delivery.agent.field, value: agent });
          }}
        />
        <span className="fe-hint">{delivery.agent.hint}</span>
      </div>
      {skill !== null && (
        <DeliverySkillField key={skill.value} skill={skill} focus={focusSkill} onEdit={onEdit} />
      )}
      <p className="fe-hint">{delivery.note}</p>
    </aside>
  );
}

/** Done's PR skill: the user's own Open PR skill by name, saved when the field loses focus; empty, Otaq's own runs. */
function DeliverySkillField({
  skill,
  focus,
  onEdit,
}: {
  skill: FlowBoardDeliverySkill;
  focus: boolean;
  onEdit: (edit: FlowEditorStageEdit) => void;
}) {
  const [text, setText] = useState(skill.value);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (focus) input.current?.focus();
  }, [focus]);
  return (
    <label className="fe-field">
      <span className="fe-label">{skill.label}</span>
      <TextInput
        ref={input}
        value={text}
        label={skill.label}
        placeholder={skill.placeholder}
        mono
        onChange={setText}
        onBlur={() => {
          if (text.trim() !== skill.value) onEdit({ edit: "delivery", field: skill.field, value: text });
        }}
      />
      {skill.warning !== null && (
        <span className="fb-head-warning">
          <Icon name="triangle-alert" size={11} />
          <span>
            <CodeText text={skill.warning} />
          </span>
        </span>
      )}
      <span className="fe-hint">{skill.hint}</span>
    </label>
  );
}

/** A small box for the user's words: Ask for changes…, or what Ask the agent to write it sends. */
function AskBox({
  anchor,
  label,
  placeholder,
  initial,
  onSend,
  onClose,
}: {
  anchor: React.RefObject<HTMLElement | null>;
  label: string;
  placeholder: string;
  initial: string;
  onSend: (text: string) => void;
  onClose: () => void;
}) {
  const [text, setText] = useState(initial);
  return (
    <Popover label={label} anchor={anchor} align="end" onClose={onClose}>
      <form
        className="fb-ask"
        onSubmit={(e) => {
          e.preventDefault();
          if (text.trim() !== "") onSend(text);
        }}
      >
        <TextArea value={text} rows={3} grow label={label} placeholder={placeholder} onChange={setText} />
        <div className="fb-side-buttons">
          <Button compact variant="secondary" label="Cancel" onClick={onClose} />
          <Button
            compact
            variant="primary"
            label="Send"
            disabled={text.trim() === ""}
            onClick={() => {
              onSend(text);
            }}
          />
        </div>
      </form>
    </Popover>
  );
}
