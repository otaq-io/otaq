// The board (*24 · The two views*; UI-02, SHL-06, BRD-01, BRD-02): the toolbar (Board · List, Lanes or Group by, Fold
// empty columns, All tickets, New ticket; `Toolbar.tsx`), then the columns of the shown project's flow from
// `board.view`, refetched on the service's events, at the board's size (one width rule: every open column at least
// `min_column_width`, sharing the width when they fit, else the board scrolls sideways with the "7 more stages" pill at
// its right edge; Rust folds Dev by height). The List view (`list/List.tsx`, LST-01) draws `board.view`'s `list` in
// place of the columns: its rows press, open, drag and open their menu as cards do, and Group by Stage's groups are the
// drop targets (a row dragged onto Ready queues it); ↑/↓ walk the rows. How the board looks is
// the project's view preferences, kept by the service (SHB-05): folds, the view and the lanes change with
// `board.set_view`, then the board asks for its view again; `g b`, `g l` and ⇧⌘O / `g f` switch the view. On Files
// (FIL-01) the project Files view (`files_view/FilesView.tsx`) takes the columns' place under the same toolbar and
// strip; the columns stay mounted, hidden, so switching back lays them out at once. A drag is the UIT-04 way
// (`drag.ts`): one `board.drag` at its start, then only this page works while the pointer moves, drawing the targets'
// verdicts (Review's Approved section is a target of its own), the lifted card and the hint or reason under it; a release on an allowed target sends
// `board.drop`, whose answer may be the Park confirmation, the Approve choice (Ready for Review → Approved: nothing
// moves until 1 or 2 is picked; Esc and the card stays) or the drag-back dialog (an earlier stage, FLW-18). Esc cancels a drag. While the ticket sheet is open over the
// board (SHL-05) the board keeps its width, gets the panel's width of room after the columns and scrolls the selected
// card's column into the part the panel leaves; a click on the open card closes the panel, and so does a plain click on
// the board's own background (`background.ts`: a column's empty area, the gaps, the room after the columns; OT-3),
// unless that click closes an open menu or popover. "Show on board" in a Project agent's pane filters the board to its tickets
// (`boardAgent`, d0087), with a chip whose ✕ (or Esc) clears it.
// Hovering (or selecting) a linked card highlights its chain, with its tooltip, and Dependencies: Always keeps every
// link drawn dashed (`chain.ts`, `ChainLayer`; T-0229, DPV-02); a blocked ticket started or dragged past Ready for Dev
// opens DPV-03's dialog (`BlockedDialog`). With Lanes: Epic
// (EPL-01, `Lanes.tsx`) the columns' headers sit once on top and each epic has its lane across them, "No epic" last;
// with Lanes: None an epic child's card names its epic. The epic card, one of
// its rows and a lane's head are dragged the same way (EPC-12, EPC-14, T-0270, T-0271): Rust answers what each queues
// or takes back, and the drop sends `board.drop`, whose outcome is the queue toast or the Park-all confirmation; a row
// lifts as a small row, a lane head as its name. `g` `e` on a selected epic, child or lane head focuses the epic
// (EPC-18, T-0274): its `epic:` token goes in the filter, the other cards, lanes and Done lines dim (Q55), the arrow
// keys walk only its cards, and Esc leaves. A lane head's right click is its epic's menu.
import {
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { requestCardAction } from "../../app/cardActions";
import { openOverlay } from "../../app/overlays";
import { useShortcut } from "../../app/registry";
import { closeTicket, openTicket, useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { EpicCard, RESTING, TicketCard } from "../../cards";
import { type Anchor, Tooltip } from "../../components";
import type { ApproveChoice as Choice } from "../../generated/ApproveChoice";
import type { ApproveMenu } from "../../generated/ApproveMenu";
import type { BoardCard } from "../../generated/BoardCard";
import type { BoardColumn } from "../../generated/BoardColumn";
import type { BoardColumnId } from "../../generated/BoardColumnId";
import type { DragVerdict } from "../../generated/DragVerdict";
import type { BoardDropParams } from "../../generated/BoardDropParams";
import type { BoardDropTarget } from "../../generated/BoardDropTarget";
import type { BoardLane } from "../../generated/BoardLane";
import type { BoardParams } from "../../generated/BoardParams";
import type { BoardSetViewParams } from "../../generated/BoardSetViewParams";
import type { BoardTicketParams } from "../../generated/BoardTicketParams";
import type { BoardView } from "../../generated/BoardView";
import type { CardActionKind } from "../../generated/CardActionKind";
import type { DropOutcome } from "../../generated/DropOutcome";
import type { MenuItem } from "../../generated/MenuItem";
import type { NewToast } from "../../generated/NewToast";
import type { ParkAllDialog } from "../../generated/ParkAllDialog";
import type { ParkDialog } from "../../generated/ParkDialog";
import type { ViewChange } from "../../generated/ViewChange";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { FilesView } from "../files_view/FilesView";
import { filesViewWanted } from "../files_view/logic";
import { List, type ListHandlers } from "../list/List";
import { findRow, groupSpans, listTargets, nextRow, rowKeys } from "../list/rows";
import { NeedsYouStrip } from "../strip/Strip";
import { ApproveChoice } from "./ApproveChoice";
import { createBackgroundClick } from "./background";
import { activeChain } from "./chain";
import { ChainLayer } from "./ChainLayer";
import { CardMenu, type OpenCardMenu } from "./CardMenu";
import { cardsOf, type CardHandlers, Column, columnOfCard, walkOf } from "./Column";
import {
  BACKLOG,
  type ColumnSpan,
  createDrag,
  type DragState,
  type LiftedRow,
  liftedLane,
  liftedRow,
  moveFocus,
  revealScroll,
  targetOrder,
  verdictOf,
} from "./drag";
import { EmptyBoard } from "./EmptyBoard";
import { type LaneHandlers, Lanes, laneWalk } from "./Lanes";
import { columnsAfter, moreText, nextScroll } from "./more";
import { NoProjects } from "./NoProjects";
import { Toolbar } from "./Toolbar";
import {
  agentOf,
  boardAgent,
  boardFilter,
  clearAgentFilter,
  escapeStep,
  filterOf,
  focusEpic,
  requestSearchFocus,
} from "./filter";
import { rememberBoardScroll, rememberColumnScroll, scrollPlace } from "./prefs";

const NO_SECTIONS: readonly string[] = [];

/** The service events that change what the board shows. */
export const BOARD_EVENTS = [
  "ticket_changed",
  "card_status_changed",
  "ticket_deleted",
  "projects_changed",
  "session_state",
  "messages_waiting",
  "inbox_changed",
  "usage_changed",
  "run_changed",
  "run_stopped",
  "settings_changed",
  // A renamed Project agent: the cards it created name it (T-0243).
  "project_agent_changed",
  // An epic lead opened, resumed, ended or changed its status: the Lead row and the track's lead line (T-0289).
  "lead_changed",
  // A named agent renamed or a role default changed: the footers' agent names (T-0345).
  "agents_changed",
  // A flow change waits (Ready for Dev's "Waiting for the flow change") or applied (the columns change; FLW-06).
  "flow_changed",
];

/** Cards say how long ago their last signal came ("12s"); the board refetches this often to keep that true. */
const SIGNAL_REFRESH_MS = 15_000;

/** The board's size settles this long after a resize before it is sent (the Done rail, Dev's fold). */
const RESIZE_SETTLE_MS = 120;

/** A failed request as a toast: the service's own words. */
export function failureToast(error: unknown): NewToast {
  return { level: "error", title: asRequestError(error).message, text: null, link: null, ticket: null };
}

function toast(error: unknown, ask: typeof request = request) {
  ask("notices.toast", failureToast(error)).catch(() => undefined);
}

/** The board's size, settled. */
function useSize(element: HTMLElement | null): { width: number; height: number } | null {
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);
  useLayoutEffect(() => {
    if (!element) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let first = true;
    const apply = () => {
      setSize((old) => {
        const next = { width: element.clientWidth, height: element.clientHeight };
        return old?.width === next.width && old.height === next.height ? old : next;
      });
    };
    // The observer reports the size once at the start (taken at once), then on each resize (taken once settled).
    const observer = new ResizeObserver(() => {
      clearTimeout(timer);
      if (first) {
        first = false;
        apply();
        return;
      }
      timer = setTimeout(apply, RESIZE_SETTLE_MS);
    });
    observer.observe(element);
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [element]);
  return size;
}

/** Refetches `load`'s view on a timer too, for the cards' signal ages. */
function useTick(ms: number): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => {
      setTick((t) => t + 1);
    }, ms);
    return () => {
      clearInterval(timer);
    };
  }, [ms]);
  return tick;
}

/** Opens the Park confirmation; its button sends `board.park` (CLS-06). */
export function confirmPark(dialog: ParkDialog, ask: typeof request = request): void {
  openOverlay("confirm", {
    title: dialog.title,
    body: dialog.message,
    rows: dialog.kept,
    confirmLabel: dialog.confirm,
    confirmIcon: dialog.confirm_icon,
    destructive: false,
    onConfirm: () => {
      const params: BoardTicketParams = { key: dialog.key };
      ask("board.park", params).catch((error: unknown) => {
        toast(error, ask);
      });
    },
  });
}

/**
 * Opens the Park-all confirmation of an epic's track dropped on Backlog (state 11); Cancel is the default and leaves
 * everything running, its button sends `board.park_all` (EPC-14, d0052).
 */
export function confirmParkAll(dialog: ParkAllDialog, ask: typeof request = request): void {
  openOverlay("confirm", {
    title: dialog.title,
    body: dialog.message,
    confirmLabel: dialog.confirm,
    confirmIcon: dialog.confirm_icon,
    destructive: false,
    cancelDefault: true,
    onConfirm: () => {
      const params: BoardTicketParams = { key: dialog.key };
      ask("board.park_all", params).catch((error: unknown) => {
        toast(error, ask);
      });
    },
  });
}

/** An approve choice opens its dialog (T-0226's Local merge, T-0227's Open PR / Push to PR). */
export function openApproveChoice(choice: Choice, ticket: string): void {
  switch (choice) {
    case "local_merge":
      openOverlay("local_merge", { ticket });
      return;
    case "open_pr":
    case "push_to_pr":
      openOverlay("open_pr", { ticket });
      return;
  }
}

/** What happens after a drop, as Rust answered it. */
export function afterDrop(
  outcome: DropOutcome,
  showApprove: (menu: ApproveMenu) => void,
  ask: typeof request = request,
): void {
  switch (outcome.kind) {
    case "nothing":
    case "moved":
      return;
    case "park":
      confirmPark(outcome.dialog, ask);
      return;
    case "park_all":
      confirmParkAll(outcome.dialog, ask);
      return;
    case "approve":
      // One way to deliver: its dialog at once, without the choice (dogfood #18).
      if (outcome.menu.direct !== null) openApproveChoice(outcome.menu.direct, outcome.menu.key);
      else showApprove(outcome.menu);
      return;
    case "drag_back":
      openOverlay("drag_back", outcome.dialog);
      return;
    // A blocked ticket (DPV-03): it already waits in Ready for Dev; the dialog says why it can't start.
    case "blocked":
      openOverlay("blocked", outcome.dialog);
      return;
    case "not_yet": {
      const notice: NewToast = {
        level: "success",
        title: outcome.message,
        text: null,
        link: null,
        ticket: null,
      };
      ask("notices.toast", notice).catch(() => undefined);
      return;
    }
  }
}

/** The verdict of `column`'s section with a target of its own (Review's Approved) while a card is dragged. */
function sectionDrag(
  column: BoardColumn,
  drag: DragState | null,
): { verdict: DragVerdict | undefined; over: boolean } | undefined {
  const target = column.sections.find((s) => s.target !== null)?.target;
  if (!drag || target == null) return undefined;
  return { verdict: verdictOf(drag.answer, target), over: drag.over === target };
}

/** Sends a drop to Rust and carries out its answer. */
export function dropCard(
  key: string,
  target: BoardDropTarget,
  showApprove: (menu: ApproveMenu) => void,
  ask: typeof request = request,
): Promise<void> {
  const params: BoardDropParams = { key, target };
  return ask("board.drop", params).then(
    (outcome) => {
      afterDrop(outcome as DropOutcome, showApprove, ask);
    },
    (error: unknown) => {
      toast(error, ask);
    },
  );
}

/** The sheet's Start (`board.start`): a drop on the first agent column, as ⌘D; a blocked ticket opens DPV-03's
 * dialog. */
export function startTicket(key: string, ask: typeof request = request): Promise<void> {
  return ask("board.start", { key }).then(
    (outcome) => {
      afterDrop(outcome as DropOutcome, () => undefined, ask);
    },
    (error: unknown) => {
      toast(error, ask);
    },
  );
}

/** A row of the epic card while it is dragged out (*03 → epic flows 3 → 8 · Dragged row*). */
function LiftedChild({ row }: { row: LiftedRow }) {
  return (
    <div className="board-ghost-row">
      <Icon name="grip-vertical" size={12} />
      <span className="board-ghost-row-key">{row.key}</span>
      <span className="board-ghost-row-title">{row.title}</span>
    </div>
  );
}

/** An epic while it is dragged by its lane's head (*03 → epic flows 1 → 3 · Dragged track handle*). */
function LiftedLane({ lane }: { lane: BoardLane }) {
  return (
    <div className="board-ghost-lane">
      <span className="board-ghost-lane-icon">
        <Icon name="layers" size={12} />
      </span>
      <span className="board-ghost-lane-name">{lane.head.name}</span>
      <span className="board-ghost-lane-detail">{lane.head.detail}</span>
    </div>
  );
}

function Lifted({ card }: { card: BoardCard }) {
  const state = { ...RESTING, dragging: true };
  return card.kind === "ticket" ? (
    <TicketCard card={card.card} state={state} />
  ) : (
    <EpicCard card={card.card} state={state} collapsed />
  );
}

/** The part of the board an element covers on its right (the panel over the board), in px; 0 when it doesn't. */
export function coveredWidth(board: { right: number }, cover: { left: number } | null): number {
  return cover === null ? 0 : Math.max(0, Math.round(board.right - cover.left));
}

/** How much of the board the open panel covers, followed as either resizes; 0 while no ticket is open. */
function useCovered(board: HTMLElement | null, open: boolean): number {
  const [covered, setCovered] = useState(0);
  useLayoutEffect(() => {
    // The frame's panel over the main area (T-0327); the gallery and the tests have none.
    const panel = open
      ? board?.closest(".frame-main")?.querySelector<HTMLElement>(":scope > .frame-sheet")
      : null;
    // The ticket sheet floats over the board, which stays as it was under it (SHT-01, T-0464).
    if (!board || !panel || panel.querySelector(":scope > .sheet")) return;
    const measure = () => {
      setCovered(coveredWidth(board.getBoundingClientRect(), panel.getBoundingClientRect()));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(board);
    observer.observe(panel);
    return () => {
      observer.disconnect();
      setCovered(0);
    };
  }, [board, open]);
  return covered;
}

/** The chip of the agent filter (d0087): "Created or changed by Checkout slowness ✕". */
function AgentChip({ label, clear, onClear }: { label: string; clear: string; onClear: () => void }) {
  return (
    <div className="board-agent-filter">
      <span className="board-agent-chip">
        <Icon name="bot" size={12} />
        <span className="board-agent-chip-label">{label}</span>
        <Tooltip text={clear}>
          <button type="button" className="board-agent-chip-clear" aria-label={clear} onClick={onClear}>
            <Icon name="x" size={12} />
          </button>
        </Tooltip>
      </span>
    </div>
  );
}

export function Board() {
  const { project, ticket: selected } = useSelection();
  const filter = filterOf(useStore(boardFilter), project);
  const agent = agentOf(useStore(boardAgent), project);
  const [element, setElement] = useState<HTMLDivElement | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const boardEl = useRef<HTMLDivElement | null>(null);
  const size = useSize(element);
  const tick = useTick(SIGNAL_REFRESH_MS);
  // Bumped after each view change the service stored: the board asks for its view again.
  const [viewVersion, setViewVersion] = useState(0);

  const load = useCallback(() => {
    if (project === null || size === null) return new Promise<never>(() => undefined);
    const params: BoardParams = {
      project,
      width: size.width,
      height: size.height,
      filter: { tokens: filter.tokens, text: filter.text },
      agent,
    };
    return (request("board.view", params) as Promise<BoardView>).then((board) => ({ project, board }));
    // `tick` refetches for the signal ages, `viewVersion` after a view change.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- tick and viewVersion are refetch triggers, not read
  }, [project, size, filter, agent, tick, viewVersion]);
  const loaded = useServiceView(load, BOARD_EVENTS).view;
  // Another project's board is never drawn while the shown one loads.
  const view = loaded?.project === project ? loaded.board : undefined;
  const foldedSections = view?.prefs.folded_sections ?? NO_SECTIONS;

  // One change of the view preferences (SHB-05), stored by the service; then the view again.
  const changeView = useCallback(
    (change: ViewChange) => {
      if (project === null) return;
      const params: BoardSetViewParams = { project, change };
      request("board.set_view", params).then(
        () => {
          setViewVersion((v) => v + 1);
        },
        (error: unknown) => {
          toast(error);
        },
      );
    },
    [project],
  );
  // `g b`, `g l` (BRD-01) and ⇧⌘O / `g f` (FIL-11): the view, for this project.
  const showView = useCallback(
    (kind: "board" | "list" | "files") => {
      if (project === null) return false;
      if (view?.prefs.view !== kind) changeView({ kind: "view", view: kind });
      return true;
    },
    [project, view, changeView],
  );
  useShortcut(
    "show_board",
    useCallback(() => showView("board"), [showView]),
  );
  useShortcut(
    "show_list",
    useCallback(() => showView("list"), [showView]),
  );
  useShortcut(
    "show_files",
    useCallback(() => showView("files"), [showView]),
  );
  const filesShown = view?.prefs.view === "files";
  // A terminal's path link asked for this project's Files view (d0117 point 8).
  const wanted = useStore(filesViewWanted);
  const wantedShown = useRef(0);
  useEffect(() => {
    if (wanted === null || wanted.project !== project || wanted.n === wantedShown.current || !view) return;
    wantedShown.current = wanted.n;
    showView("files");
  }, [wanted, project, view, showView]);

  // The drag: state for what the columns draw, the pointer only in a ref (the lifted card follows it unrendered).
  const [drag, setDrag] = useState<DragState | null>(null);
  const [approve, setApprove] = useState<{ menu: ApproveMenu; anchor: Anchor } | null>(null);
  const [cardMenu, setCardMenu] = useState<OpenCardMenu | null>(null);
  const ghost = useRef<HTMLDivElement>(null);
  const columns = useRef(new Map<BoardDropTarget, HTMLElement>());
  // The List's groups that take a drop (LST-02), measured like the columns when a drag starts.
  const listGroups = useRef(new Map<BoardDropTarget, HTMLElement>());
  const list = view?.list ?? null;
  const listRef = useRef(list);
  const listKeys = useMemo(() => (list === null ? [] : rowKeys(list)), [list]);
  const listKeysRef = useRef(listKeys);
  const listTargetsRef = useRef<BoardDropTarget[]>([]);
  useLayoutEffect(() => {
    listRef.current = list;
    listKeysRef.current = listKeys;
    listTargetsRef.current = list === null ? [] : listTargets(list);
  }, [list, listKeys]);
  const pointer = useRef({ x: 0, y: 0 });
  // The sections that are drop targets of their own (Review's Approved), with the column each is in (STG-08).
  const sectionOwners = useMemo(
    () =>
      new Map(
        (view?.columns ?? []).flatMap((c) =>
          c.sections.flatMap((s): [BoardDropTarget, BoardDropTarget][] =>
            s.target === null ? [] : [[s.target, c.id]],
          ),
        ),
      ),
    [view],
  );
  const sectionOwnersRef = useRef(sectionOwners);
  useLayoutEffect(() => {
    sectionOwnersRef.current = sectionOwners;
  }, [sectionOwners]);

  // The approve choice opens over the section it approves into (Approved), else its column.
  const showApprove = useCallback((menu: ApproveMenu) => {
    const [section] = [...sectionOwnersRef.current.entries()];
    const approved = (
      section &&
      (columns.current.get(section[0]) ?? columns.current.get(section[1]))
    )?.getBoundingClientRect();
    const anchor = approved
      ? { left: approved.left + 8, top: approved.top + 40, width: approved.width - 16, height: 0 }
      : { left: pointer.current.x, top: pointer.current.y, width: 0, height: 0 };
    setApprove({ menu, anchor });
  }, []);

  const controller = useMemo(
    () =>
      createDrag(request, {
        onState: setDrag,
        onError: (error) => {
          toast(error);
        },
      }),
    [],
  );

  const placeGhost = useCallback((x: number, y: number) => {
    pointer.current = { x, y };
    if (ghost.current) ghost.current.style.transform = `translate(${String(x)}px, ${String(y)}px)`;
  }, []);

  // A section that is a target of its own (Review's Approved) is one from its label down to its column's bottom,
  // where the cards scroll.
  const spans = useCallback(
    (): ColumnSpan[] =>
      listRef.current !== null
        ? groupSpans(listGroups.current)
        : [...columns.current.entries()].map(([id, el]) => {
            const r = el.getBoundingClientRect();
            const owner = sectionOwnersRef.current.get(id);
            if (owner === undefined) return { id, left: r.left, right: r.right };
            const column = columns.current.get(owner)?.getBoundingClientRect();
            const cards = el.closest(".board-column-cards")?.getBoundingClientRect();
            return {
              id,
              left: r.left,
              right: r.right,
              top: Math.max(r.top, cards?.top ?? r.top),
              bottom: column?.bottom ?? r.bottom,
            };
          }),
    [],
  );

  // A click on a card opens it; on the open card it closes the panel (SHL-05).
  const selectedRef = useRef(selected);
  useLayoutEffect(() => {
    selectedRef.current = selected;
  }, [selected]);
  const release = useCallback(() => {
    const done = controller.release();
    if (done.kind === "click") {
      if (done.key === selectedRef.current) closeTicket();
      else openTicket(done.key);
    }
    if (done.kind === "drop") void dropCard(done.key, done.target, showApprove);
  }, [controller, showApprove]);
  // A plain click on the board's background closes the panel too (OT-3).
  const background = useMemo(
    () =>
      createBackgroundClick(() => {
        closeTicket();
      }),
    [],
  );

  // The pointer is followed on the window while pressed, so a drag keeps going over anything.
  useEffect(() => {
    const move = (e: globalThis.PointerEvent) => {
      if (!controller.move(e.clientX, e.clientY, spans)) return;
      e.preventDefault();
      placeGhost(e.clientX, e.clientY);
    };
    const up = (e: globalThis.PointerEvent) => {
      release();
      background.release(e);
    };
    const lost = () => {
      controller.cancel();
      background.cancel();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", lost);
    window.addEventListener("blur", lost);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", lost);
      window.removeEventListener("blur", lost);
    };
  }, [controller, spans, release, placeGhost, background]);

  // The focused epic (EPC-18): read by Esc without re-registering it.
  const focus = view?.focus ?? null;
  const focusRef = useRef(focus);
  useLayoutEffect(() => {
    focusRef.current = focus;
  }, [focus]);
  const focusOn = useCallback(
    (key: string | null) => {
      if (project === null) return;
      focusEpic(project, key).catch((error: unknown) => {
        toast(error);
      });
    },
    [project],
  );

  // Esc while dragging: the card flies back (before the panel's Close takes the key). Otherwise Esc leaves a
  // focused epic (its token leaves the filter), then clears the agent filter (`escapeStep`).
  const agentRef = useRef(agent);
  useLayoutEffect(() => {
    agentRef.current = agent;
  }, [agent]);
  const cancelDrag = useCallback(() => {
    switch (escapeStep(controller.state() !== null, focusRef.current !== null, agentRef.current !== null)) {
      case "drag":
        controller.cancel();
        return true;
      case "focus":
        focusOn(null);
        return true;
      case "agent":
        if (project !== null) clearAgentFilter(project);
        return true;
      case null:
        return false;
    }
  }, [controller, focusOn, project]);
  useShortcut("close_panel", cancelDrag);

  // `g` `e`: focus the epic of the lane head with the keyboard, else of the selected card. A standalone ticket has
  // none: Rust answers nothing and the filter stays.
  const focusSelected = useCallback(() => {
    const name = document.activeElement?.closest<HTMLElement>(".board-lane-head[data-epic]");
    const key = name?.dataset.epic ?? selected;
    if (key === null) return false;
    focusOn(key);
    return true;
  }, [selected, focusOn]);
  useShortcut("focus_epic", focusSelected);

  const order = useMemo(() => view?.columns.map((c) => c.id) ?? [], [view]);
  const targets = useMemo(() => targetOrder(view?.columns ?? []), [view]);
  // The arrow keys walk the cards; with an epic focused, only its cards (EPC-18); never those of a folded section.
  const keys = useMemo(() => {
    const strong = view?.focus ? new Set(view.focus.keys) : null;
    if (view?.lanes != null)
      return laneWalk(view.columns, view.lanes).map((keys) =>
        keys.filter((key) => strong === null || strong.has(key)),
      );
    return (
      view?.columns.map((c) =>
        walkOf(c, foldedSections)
          .map((card) => card.card.key)
          .filter((key) => strong === null || strong.has(key)),
      ) ?? []
    );
  }, [view, foldedSections]);
  const keysRef = useRef(keys);
  const orderRef = useRef(order);
  const targetsRef = useRef(targets);
  useLayoutEffect(() => {
    keysRef.current = keys;
    orderRef.current = order;
    targetsRef.current = targets;
  }, [keys, order, targets]);

  // The right-click menu: its items are asked when it opens, so they follow the ticket as it is now.
  const openMenu = useCallback(
    (key: string, at: { left: number; top: number }) => {
      if (controller.state() !== null) return;
      const params: BoardTicketParams = { key };
      (request("board.menu", params) as Promise<MenuItem[]>).then(
        (items) => {
          setCardMenu({ key, items, anchor: { left: at.left, top: at.top, width: 0, height: 0 } });
        },
        (error: unknown) => {
          toast(error);
        },
      );
    },
    [controller],
  );

  const handlers: CardHandlers = useMemo(
    () => ({
      press: (key, _column, e: PointerEvent) => {
        // Buttons on the card (actions, options, chips) keep their own clicks.
        if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
        controller.press(key, e.clientX, e.clientY);
        placeGhost(e.clientX, e.clientY);
      },
      pressChild: (key, e: PointerEvent) => {
        if (e.button !== 0) return;
        controller.press(key, e.clientX, e.clientY);
        placeGhost(e.clientX, e.clientY);
      },
      key: (key, column, e: KeyboardEvent) => {
        if (e.target !== e.currentTarget) return;
        const lifted = controller.state();
        if (e.key === " ") {
          e.preventDefault();
          if (lifted === null) controller.lift(key, column);
          else release();
          return;
        }
        if (e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "ArrowUp" || e.key === "ArrowDown") {
          e.preventDefault();
          if (lifted !== null) {
            if (e.key === "ArrowLeft" || e.key === "ArrowRight")
              controller.step(e.key === "ArrowLeft" ? -1 : 1, targetsRef.current);
            return;
          }
          const col = orderRef.current.indexOf(column);
          const row = keysRef.current[col]?.indexOf(key) ?? -1;
          const next = moveFocus(keysRef.current, col, row, e.key);
          const target = next && keysRef.current[next.col]?.[next.row];
          if (target) document.querySelector<HTMLElement>(`[data-card="${CSS.escape(target)}"]`)?.focus();
          return;
        }
        if (e.key === "Enter") {
          e.preventDefault();
          openTicket(key);
          return;
        }
        // The keyboard's menu key (or ⇧F10) opens the right-click menu under the card.
        if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
          e.preventDefault();
          const r = e.currentTarget.getBoundingClientRect();
          openMenu(key, { left: r.left + 12, top: r.top + 24 });
        }
      },
      open: (key) => {
        openTicket(key);
      },
      action: (key, kind: CardActionKind) => {
        openTicket(key);
        requestCardAction(key, kind);
      },
      addToEpic: (epic) => {
        openOverlay("new_ticket", { epic });
      },
      menu: openMenu,
      hover: (key, over) => {
        setHovered((old) => (over ? key : old === key ? null : old));
      },
    }),
    [controller, release, placeGhost, openMenu],
  );

  // ⌘D: start the selected ticket, as if it was dropped on Dev (the first agent column). On an epic (its card, or its
  // track's name) it queues the children still in Backlog; on a child row only that child (EPC-12, d0058).
  const startOn = view?.columns.find((c) => c.kind === "agent")?.id;
  const startSelected = useCallback(() => {
    if (selected === null || startOn === undefined) return false;
    void dropCard(selected, startOn, showApprove);
    return true;
  }, [selected, startOn, showApprove]);
  useShortcut("start_dev", startSelected);
  // Park (0097, palette only unless the user gives it a key): the selected ticket as if dropped on Backlog, which
  // asks the Park confirmation (CLS-06).
  const parkSelected = useCallback(() => {
    if (selected === null) return false;
    void dropCard(selected, BACKLOG, showApprove);
    return true;
  }, [selected, showApprove]);
  useShortcut("park", parkSelected);

  const element$ = useCallback((id: BoardDropTarget, el: HTMLElement | null) => {
    if (el) columns.current.set(id, el);
    else columns.current.delete(id);
  }, []);
  const group$ = useCallback((id: BoardDropTarget, el: HTMLElement | null) => {
    if (el) listGroups.current.set(id, el);
    else listGroups.current.delete(id);
  }, []);

  // A List row does what a card does (LST-02): a press is a click or a drag, its action opens the ticket and asks for
  // it, Start is ⌘D's drop, the menu is the card's. Space lifts it onto its own group (Group by Stage), ↑/↓ then move
  // it between the groups that take it, and walk the rows otherwise.
  const listHandlers: ListHandlers = useMemo(
    () => ({
      press: (key, e: PointerEvent) => {
        if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
        controller.press(key, e.clientX, e.clientY);
        placeGhost(e.clientX, e.clientY);
      },
      key: (key, target, e: KeyboardEvent) => {
        if (e.target !== e.currentTarget) return;
        const lifted = controller.state();
        if (e.key === " ") {
          e.preventDefault();
          if (lifted !== null) release();
          else if (target !== null) controller.lift(key, target);
          return;
        }
        if (e.key === "ArrowUp" || e.key === "ArrowDown") {
          e.preventDefault();
          const step = e.key === "ArrowUp" ? -1 : 1;
          if (lifted !== null) {
            controller.step(step, listTargetsRef.current);
            return;
          }
          const next = nextRow(listKeysRef.current, key, step);
          if (next !== null) document.querySelector<HTMLElement>(`[data-row="${CSS.escape(next)}"]`)?.focus();
          return;
        }
        if (e.key === "Enter") {
          e.preventDefault();
          openTicket(key);
          return;
        }
        if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
          e.preventDefault();
          const r = e.currentTarget.getBoundingClientRect();
          openMenu(key, { left: r.left + 12, top: r.top + 24 });
        }
      },
      action: (key, kind: CardActionKind) => {
        openTicket(key);
        requestCardAction(key, kind);
      },
      start: (key, target) => {
        void dropCard(key, target, showApprove);
      },
      menu: openMenu,
      open: (key) => {
        openTicket(key);
      },
    }),
    [controller, release, placeGhost, openMenu, showApprove],
  );

  const place = scrollPlace(project ?? "");
  // The board's own sideways scroll comes back with its project too.
  const hasView = view !== undefined;
  useLayoutEffect(() => {
    const el = boardEl.current;
    if (el && hasView) el.scrollLeft = scrollPlace(project ?? "").left;
  }, [element, project, hasView]);
  const onScroll = useCallback(
    (id: BoardColumnId, top: number) => {
      if (project !== null) rememberColumnScroll(project, id, top);
    },
    [project],
  );

  const draggedCard = useMemo(() => {
    if (drag === null || !view || view.list !== null) return null;
    for (const c of view.columns) {
      const found = cardsOf(c).find((card) => card.card.key === drag.key);
      if (found) return found;
    }
    return null;
  }, [drag, view]);
  const draggedRow = useMemo(() => {
    if (drag === null || !view || draggedCard !== null) return null;
    // A List row lifts as a small row with its key and title.
    if (view.list !== null) {
      const found = findRow(view.list, drag.key);
      return found ? { key: drag.key, title: found.row.card.title } : null;
    }
    const epics = view.columns.flatMap((c) =>
      cardsOf(c).flatMap((card) => (card.kind === "epic" ? [card.card] : [])),
    );
    return liftedRow(drag.key, epics);
  }, [drag, view, draggedCard]);
  const draggedLane = useMemo(
    () => (drag === null || !view || draggedCard !== null ? null : liftedLane(drag.key, view.lanes ?? [])),
    [drag, view, draggedCard],
  );

  // A lane head is its epic (EPL-01): pressed, it may be dragged like the epic's card (a short press, or ↵, opens the
  // epic); its right click is the epic's menu.
  const laneHandlers: LaneHandlers = useMemo(
    () => ({
      press: (epic, e) => {
        controller.press(epic, e.clientX, e.clientY);
        placeGhost(e.clientX, e.clientY);
      },
      menu: openMenu,
      open: (epic) => {
        openTicket(epic);
      },
    }),
    [controller, placeGhost, openMenu],
  );

  // An epic's items in its menu do what the board does: a drop, and the focus.
  const menuBoard = useMemo(
    () => ({
      drop: (key: string, column: BoardDropTarget) => {
        void dropCard(key, column, showApprove);
      },
      focus: focusOn,
    }),
    [showApprove, focusOn],
  );

  const overVerdict = drag ? verdictOf(drag.answer, drag.over) : undefined;
  // Over Approved (a section with a target of its own) the hint is drawn in the section itself (*Review column · drop
  // target*).
  const ghostHint = drag?.over != null && sectionOwners.has(drag.over) ? undefined : overVerdict;

  // The panel over the board (SHL-05): room after the columns to scroll into, and the selected card's column
  // scrolled into the part the panel leaves (*x9qiI*), when it opens, moves to another card or is resized.
  const covered = useCovered(element, selected !== null);
  const selectedColumn = view && selected !== null ? columnOfCard(view.columns, selected) : null;
  useLayoutEffect(() => {
    const board = boardEl.current;
    const column = selectedColumn === null ? undefined : columns.current.get(selectedColumn);
    if (!board || !column || covered === 0) return;
    const b = board.getBoundingClientRect();
    const pad = parseFloat(getComputedStyle(board).paddingLeft) || 0;
    const next = revealScroll(
      column.getBoundingClientRect(),
      { left: b.left + pad, right: b.right - covered - pad },
      board.scrollLeft,
    );
    if (next !== null) board.scrollLeft = next;
  }, [selected, selectedColumn, covered]);

  // The "7 more stages" pill (BRD-02): the columns cut off at the board's right edge, again as it scrolls or resizes.
  const [more, setMore] = useState<string | null>(null);
  const moreWords = view?.more_stages ?? null;
  const spansInBoard = useCallback(() => {
    const board = boardEl.current;
    if (!board) return null;
    const b = board.getBoundingClientRect();
    const pad = parseFloat(getComputedStyle(board).paddingLeft) || 0;
    const spans = [
      ...board.querySelectorAll<HTMLElement>(
        ".board-columns > [data-column], .board-lane-headers > [data-column]",
      ),
    ].map((el) => {
      const r = el.getBoundingClientRect();
      return { left: r.left, right: r.right };
    });
    return { spans, b, pad };
  }, []);
  const measureMore = useCallback(() => {
    const measured = spansInBoard();
    if (!measured || moreWords === null) return;
    const after = columnsAfter(measured.spans, measured.b.right - covered - measured.pad);
    const next = moreText(moreWords, after.length);
    setMore((old) => (old === next ? old : next));
  }, [spansInBoard, moreWords, covered]);
  useLayoutEffect(() => {
    // After the columns are laid out.
    const frame = requestAnimationFrame(measureMore);
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [measureMore, size, view]);
  const scrollToMore = useCallback(() => {
    const measured = spansInBoard();
    const board = boardEl.current;
    if (!measured || !board) return;
    const after = columnsAfter(measured.spans, measured.b.right - covered - measured.pad);
    const left = nextScroll(after, measured.b.left, board.scrollLeft, measured.pad);
    if (left !== null) board.scrollTo({ left, behavior: "smooth" });
  }, [spansInBoard, covered]);

  // The columns' widths and their least total (for the room after them), from the view (SHL-01).
  const boardStyle = useMemo(() => {
    if (!view) return undefined;
    // Folded Done is a line under the last column, not a column.
    const shown = view.columns.filter((c) => !(c.collapsed && c.rail === null));
    const rails = shown.filter((c) => c.collapsed).length;
    return {
      "--column-width": `${String(view.min_column_width)}px`,
      "--rail-width": `${String(view.folded_width)}px`,
      "--open-columns": String(shown.length - rails),
      "--rail-columns": String(rails),
    } as CSSProperties;
  }, [view]);
  // The last open column carries Done's line while Done is folded.
  const lastOpen = view?.columns.findLast((c) => !c.collapsed)?.id ?? null;
  const chain = useMemo(
    () => activeChain(view?.chains ?? [], hovered, selected, drag !== null),
    [view, hovered, selected, drag],
  );

  // Keyboard drags draw the lifted card over the column it is in.
  useLayoutEffect(() => {
    if (!drag?.keyboard || drag.over === null || !ghost.current) return;
    const r = columns.current.get(drag.over)?.getBoundingClientRect();
    if (r) ghost.current.style.transform = `translate(${String(r.left + 24)}px, ${String(r.top + 80)}px)`;
  }, [drag]);

  return (
    <div className="board-screen">
      {view && (
        <Toolbar
          toolbar={view.toolbar}
          onView={changeView}
          onAllTickets={requestSearchFocus}
          onNewTicket={() => {
            openOverlay("new_ticket");
          }}
        />
      )}
      <NeedsYouStrip />
      {filesShown && project !== null && <FilesView project={project} />}
      <div
        className="board"
        hidden={filesShown}
        data-view={list === null ? undefined : "list"}
        ref={(el) => {
          boardEl.current = el;
          setElement(el);
        }}
        data-dragging={drag !== null || undefined}
        style={boardStyle}
        onScroll={(e) => {
          if (project !== null) rememberBoardScroll(project, e.currentTarget.scrollLeft);
          measureMore();
        }}
        // A card that moved away under the pointer never reports leaving; leaving the board ends its highlight.
        onPointerLeave={() => {
          setHovered(null);
        }}
        onPointerDown={(e) => {
          background.press(e);
        }}
      >
        {view?.agent_filter && (
          <AgentChip
            label={view.agent_filter.label}
            clear={view.agent_filter.clear}
            onClear={() => {
              if (project !== null) clearAgentFilter(project);
            }}
          />
        )}
        {list !== null ? (
          <List
            list={list}
            selected={selected}
            lifted={drag?.key ?? null}
            verdict={(target) => (drag ? verdictOf(drag.answer, target) : undefined)}
            handlers={listHandlers}
            group={group$}
            onView={changeView}
          />
        ) : view?.lanes != null ? (
          <Lanes
            columns={view.columns}
            lanes={view.lanes}
            verdictOf={(id) => (drag ? verdictOf(drag.answer, id) : undefined)}
            dragging={drag !== null}
            selected={selected}
            chain={chain}
            lifted={drag?.key ?? null}
            focus={focus}
            handlers={handlers}
            laneHandlers={laneHandlers}
            onView={changeView}
            onAdd={() => {
              openOverlay("new_ticket");
            }}
            doneLine={view.done}
            element={element$}
          />
        ) : (
          <div className="board-columns">
            {view?.columns.map((column) => (
              <Column
                key={`${project ?? ""}:${column.id}`}
                column={column}
                chipWords={view.scroll_chip}
                verdict={drag ? verdictOf(drag.answer, column.id) : undefined}
                dragging={drag !== null}
                selected={selected}
                chain={chain}
                lifted={drag?.key ?? null}
                focus={focus}
                handlers={handlers}
                showEmptyHint={!view.empty}
                scrollTop={place.columns[column.id] ?? 0}
                onScroll={onScroll}
                onView={changeView}
                onAdd={() => {
                  openOverlay("new_ticket");
                }}
                foldedSections={foldedSections}
                doneLine={column.id === lastOpen ? view.done : null}
                approved={sectionDrag(column, drag)}
                element={element$}
              />
            ))}
            {covered > 0 && <div className="board-under-panel" style={{ width: covered }} />}
          </div>
        )}
        {view?.empty && <EmptyBoard view={view.empty} />}
        {project === null && <NoProjects />}
        {list === null && (
          <ChainLayer
            chain={chain}
            links={view?.links ?? []}
            hovered={hovered}
            board={element}
            layout={view}
          />
        )}
        <div className="board-ghost" ref={ghost} hidden={drag === null}>
          {draggedCard && (
            <div className="board-ghost-card">
              <Lifted card={draggedCard} />
            </div>
          )}
          {draggedRow && <LiftedChild row={draggedRow} />}
          {draggedLane && <LiftedLane lane={draggedLane} />}
          {ghostHint?.kind === "allowed" && ghostHint.hint !== "" && (
            <div className="board-drop-hint">
              <Icon name={ghostHint.icon} size={11} />
              <span className="board-drop-hint-text">
                <span>{ghostHint.hint}</span>
                {ghostHint.note !== null && <span className="board-drop-hint-note">{ghostHint.note}</span>}
              </span>
            </div>
          )}
          {overVerdict?.kind === "forbidden" && (
            <div className="board-not-allowed" role="status">
              <Icon name="ban" size={18} />
              <span className="board-not-allowed-title">{overVerdict.title}</span>
              {overVerdict.detail !== null && (
                <span className="board-not-allowed-detail">{overVerdict.detail}</span>
              )}
            </div>
          )}
          {overVerdict && overVerdict.kind !== "home" && (
            <span className="board-cursor" data-verdict={overVerdict.kind}>
              <Icon name={overVerdict.kind === "allowed" ? "plus" : "ban"} size={12} />
            </span>
          )}
        </div>
        {cardMenu && (
          <CardMenu
            menu={cardMenu}
            board={menuBoard}
            onError={(error) => {
              toast(error);
            }}
            onClose={() => {
              setCardMenu(null);
            }}
          />
        )}
        {approve && (
          <ApproveChoice
            menu={approve.menu}
            anchor={approve.anchor}
            onChoose={(choice) => {
              setApprove(null);
              openApproveChoice(choice, approve.menu.key);
            }}
            onCancel={() => {
              setApprove(null);
            }}
          />
        )}
      </div>
      {more !== null && !filesShown && (
        <button
          type="button"
          className="board-more-stages"
          style={{ right: covered + 16 }}
          onClick={scrollToMore}
        >
          <span>{more}</span>
          <Icon name="arrow-right" size={12} />
        </button>
      )}
    </div>
  );
}
