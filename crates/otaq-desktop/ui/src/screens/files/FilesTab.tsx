// The ticket sheet's Files tab (FTB-01…FTB-05, SHT-06; T-0468; `s25-07`, `s25-08`, `s25-09`). On the left the source
// ("SHOP-22’s worktree · otaq/SHOP-22 · from main"), the search box (⌥⌘P: matches flat with their folder, ↑ ↓,
// Return opens, Esc clears), All files · Changed · N, and the tree folder by folder or the changed files. On the right
// a file as File or Diff with New ticket from this file, or All changes: every changed file's diff in one scroll,
// Unified · Split, Open file, with line comments while the ticket waits in your check. What it says is Rust's
// (`src/api/files.rs`, `otaq-view`'s `files_tab.rs`); it keeps the moment of interaction only.
import {
  type ReactNode,
  type RefObject,
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { openOverlay } from "../../app/overlays";
import type { TicketProps } from "../../app/registry";
import { openTicket } from "../../app/selection";
import { createStore, useStore } from "../../app/store";
import { Button, DecisionsList, Kbd, Segmented, PLAIN_TEXT } from "../../components";
import type { DecisionItemView } from "../../generated/DecisionItemView";
import type { DecisionsSection } from "../../generated/DecisionsSection";
import type { DiffFileView } from "../../generated/DiffFileView";
import type { FilesChangeRow } from "../../generated/FilesChangeRow";
import type { FilesChangesView } from "../../generated/FilesChangesView";
import type { FilesChip } from "../../generated/FilesChip";
import type { FilesComments } from "../../generated/FilesComments";
import type { FilesFilePage } from "../../generated/FilesFilePage";
import type { FilesFilter } from "../../generated/FilesFilter";
import type { FilesHitPart } from "../../generated/FilesHitPart";
import type { FilesLoad } from "../../generated/FilesLoad";
import type { FilesMark } from "../../generated/FilesMark";
import type { FilesReviewNotes } from "../../generated/FilesReviewNotes";
import type { FilesSearchView } from "../../generated/FilesSearchView";
import type { FilesTabView } from "../../generated/FilesTabView";
import type { FilesTag } from "../../generated/FilesTag";
import type { FilesTreeRow } from "../../generated/FilesTreeRow";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { Report } from "../sections/Report";
import { type CommentContext, CodeLineRow, Hunk, useComments, useScrollToLine } from "./Hunks";
import {
  type FilesState,
  LINE_HEIGHT,
  NOTHING_OPEN,
  type OpenFile,
  chunks,
  openFile,
  pathParts,
  searchFocus,
  showsLines,
  showsSideBySide,
  shownFilter,
  shownMode,
  steppedRow,
  toggled,
  visibleRows,
} from "./logic";

/** The service events that change the files (a new round, a merge, the agent's work) and the comments. */
export const FILES_EVENTS = ["card_status_changed", "ticket_changed"] as const;

const ignore = () => undefined;

/** A changed file's section in All changes: its path, and the running child's key for an epic child's change on top
 * (Rust's `FilesChangeRow.id`). */
function changeId(row: FilesTreeRow): string {
  return row.child === null ? row.path : `${row.path}\u0000${row.child}`;
}

/** The last ⌥⌘P a search box took. */
let focusHandled = 0;

/** Each ticket's tab state while the app runs, so leaving the tab and coming back keeps the open file. */
const tabStates = createStore<ReadonlyMap<string, FilesState>>(new Map());

/**
 * Opens `ticket`'s sheet on its Files tab with `file` open (the project Files view's Open on Files, FIL-05): the
 * tab's state first, then the sheet.
 */
export function openFileIn(ticket: string, file: OpenFile): void {
  tabStates.update((m) => new Map(m).set(ticket, openFile(m.get(ticket) ?? NOTHING_OPEN, file)));
  openTicket(ticket, "files");
}

function useTabState(ticket: string): [FilesState, (change: (s: FilesState) => FilesState) => void] {
  const all = useStore(tabStates);
  const state = all.get(ticket) ?? NOTHING_OPEN;
  const update = useCallback(
    (change: (s: FilesState) => FilesState) => {
      tabStates.update((m) => new Map(m).set(ticket, change(m.get(ticket) ?? NOTHING_OPEN)));
    },
    [ticket],
  );
  return [state, update];
}

/** The element's width, followed as it resizes (Split depends on the pane's). */
export function useWidth<T extends HTMLElement>(): [RefObject<T | null>, number] {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      setWidth(el.clientWidth);
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, []);
  return [ref, width];
}

/** A path that may wrap after each "/" (as the frame's long names do), not inside a name. */
export function Path({ text }: { text: string }) {
  return (
    <>
      {pathParts(text).map((part, i) => (
        <span key={i}>
          {i > 0 && <wbr />}
          {part}
        </span>
      ))}
    </>
  );
}

/** The project Files view's chips: one per ticket changing the file, in the ticket's colour (its bar's), then the
 * other base's tag ("release/2.3"); nothing in a ticket's tab. */
function Chips({ chips, branch }: { chips: readonly FilesChip[]; branch: string | null }) {
  return (
    <>
      {chips.map((c) => (
        <span key={c.key} className="files-chip">
          <span className="files-chip-dot" style={{ background: `var(--${c.colour})` }} aria-hidden />
          {c.key}
        </span>
      ))}
      {branch !== null && <span className="files-branch">{branch}</span>}
    </>
  );
}

function Mark({ mark, children }: { mark: FilesMark | null; children?: ReactNode }) {
  if (!mark) return null;
  return (
    <>
      <span className="files-letter" data-status={mark.status}>
        {mark.letter}
      </span>
      {mark.uncommitted !== null && (
        <span className="files-uncommitted" title={mark.uncommitted} aria-label={mark.uncommitted} />
      )}
      <span className="files-spacer" />
      {children}
      {mark.counts !== null && <span className="files-counts">{mark.counts}</span>}
    </>
  );
}

function Tags({ tags }: { tags: readonly FilesTag[] }) {
  if (tags.length === 0) return null;
  return (
    <>
      {tags.map((t) => (
        <span key={t.label} className="files-tag" data-writing={t.writing || undefined}>
          {t.label}
        </span>
      ))}
    </>
  );
}

/** One row of the tree: a folder opens and closes, a file opens on the right; `onMenu` takes a right-click (the
 * project Files view's file menu). */
export function TreeRowButton({
  row,
  open,
  selected,
  onPress,
  onMenu,
}: {
  row: FilesTreeRow;
  open: boolean;
  selected: boolean;
  onPress: (row: FilesTreeRow) => void;
  onMenu?: (row: FilesTreeRow, at: { left: number; top: number }) => void;
}) {
  const folder = row.kind === "folder";
  return (
    <button
      type="button"
      className="files-row"
      data-quiet={row.quiet || undefined}
      data-struck={row.struck || undefined}
      data-selected={selected || undefined}
      aria-expanded={folder ? open : undefined}
      aria-current={selected || undefined}
      disabled={!row.opens}
      style={{ paddingLeft: 8 + row.depth * 14 }}
      onClick={() => {
        onPress(row);
      }}
      onContextMenu={
        onMenu &&
        ((e) => {
          e.preventDefault();
          onMenu(row, { left: e.clientX, top: e.clientY });
        })
      }
    >
      {folder ? (
        <Icon name={open ? "chevron-down" : "chevron-right"} size={12} />
      ) : (
        <span className="files-chevron-room" />
      )}
      <Icon name={folder ? (open ? "folder-open" : "folder") : "file"} size={13} />
      <span className="files-name">{row.name}</span>
      <Tags tags={row.tags} />
      {row.mark ? (
        <Mark mark={row.mark}>
          <Chips chips={row.chips} branch={row.branch} />
        </Mark>
      ) : (
        <>
          <span className="files-spacer" />
          <Chips chips={row.chips} branch={row.branch} />
          {row.changed_below !== null && <span className="files-changed-below">{row.changed_below}</span>}
        </>
      )}
    </button>
  );
}

function Parts({ parts }: { parts: readonly FilesHitPart[] }) {
  return (
    <>
      {parts.map((p, i) =>
        p.hit ? (
          <mark key={i} className="files-hit">
            {p.text}
          </mark>
        ) : (
          <span key={i}>{p.text}</span>
        ),
      )}
    </>
  );
}

/** The search's matches, flat with their folder; the selected one opens with Return. */
export function SearchResults({
  found,
  selected,
  onOpen,
}: {
  found: FilesSearchView;
  selected: number;
  /** The match's path, and the running child writing it on an epic (its worktree's version opens). */
  onOpen: (path: string, child: string | null) => void;
}) {
  return (
    <div className="files-list" role="listbox" aria-label={found.query}>
      {found.rows.map((row, i) => (
        <button
          key={row.path}
          type="button"
          role="option"
          aria-selected={i === selected}
          className="files-row files-match"
          data-quiet={row.quiet || undefined}
          data-selected={i === selected || undefined}
          onClick={() => {
            onOpen(row.path, row.child);
          }}
        >
          <Icon name="file" size={13} />
          <span className="files-match-text">
            <span className="files-name">
              <Parts parts={row.name} />
            </span>
            {row.folder.length > 0 && (
              <span className="files-folder">
                <Parts parts={row.folder} />
              </span>
            )}
          </span>
          <Tags tags={row.tags} />
          {row.mark ? (
            <Mark mark={row.mark}>
              <Chips chips={row.chips} branch={row.branch} />
            </Mark>
          ) : (
            <span className="files-spacer" />
          )}
        </button>
      ))}
      {found.empty !== null && <p className="files-quiet">{found.empty}</p>}
      {found.more !== null && <p className="files-quiet">{found.more}</p>}
    </div>
  );
}

/** The whole tree: the top folder's rows and each opened folder's, asked for as it opens. */
function Tree({
  ticket,
  view,
  state,
  onState,
}: {
  ticket: string;
  view: FilesTabView;
  state: FilesState;
  onState: (change: (s: FilesState) => FilesState) => void;
}) {
  // The opened folders' rows for this view of the tab: a new view (the work moved on) asks for them again.
  const [cache, setCache] = useState<{ view: FilesTabView; rows: ReadonlyMap<string, FilesTreeRow[]> }>({
    view,
    rows: new Map(),
  });
  const loaded = cache.view === view ? cache.rows : new Map<string, FilesTreeRow[]>();
  const keep = useCallback(
    (path: string, rows: FilesTreeRow[]) => {
      setCache((c) => ({ view, rows: new Map(c.view === view ? c.rows : []).set(path, rows) }));
    },
    [view],
  );
  useEffect(() => {
    let current = true;
    for (const path of state.open) {
      (
        request("files.tree", { key: ticket, path, depth: path.split("/").length }) as Promise<FilesTreeRow[]>
      ).then((rows) => {
        if (current) keep(path, rows);
      }, ignore);
    }
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- read once per view: a folder opened later loads on its click
  }, [ticket, keep]);
  const press = (row: FilesTreeRow) => {
    if (row.kind !== "folder") {
      // An epic's file a running child is writing opens from that child's worktree (FTB-06).
      onState((s) => openFile(s, { path: row.path, child: row.child, mode: null }));
      return;
    }
    onState((s) => ({ ...s, open: toggled(s.open, row.path) }));
    if (!state.open.has(row.path) && !loaded.has(row.path)) {
      (
        request("files.tree", { key: ticket, path: row.path, depth: row.depth + 1 }) as Promise<
          FilesTreeRow[]
        >
      ).then((rows) => {
        keep(row.path, rows);
      }, ignore);
    }
  };
  return (
    <div className="files-list" role="tree" aria-label={view.source.title}>
      {visibleRows(view.rows, state.open, loaded).map((row) => (
        <TreeRowButton
          key={row.path}
          row={row}
          open={state.open.has(row.path)}
          selected={state.file?.path === row.path}
          onPress={press}
        />
      ))}
    </div>
  );
}

/** "Agent summary" above All changes, folded by a click (d0017). */
function Notes({
  ticket,
  notes,
  decisions,
  onChangeDecision,
}: {
  ticket: string;
  notes: FilesReviewNotes | undefined;
  decisions: DecisionsSection | null | undefined;
  onChangeDecision: (item: DecisionItemView) => void;
}) {
  const [folded, setFolded] = useState(false);
  const review = notes?.review ?? null;
  const summary = notes?.summary ?? null;
  if (!review && !summary && !decisions) return null;
  return (
    <div className="files-notes">
      {review && (
        <div className="diff-entry">
          <span className="status-pill" data-palette={review.entry.palette}>
            <Icon name={review.entry.icon} size={12} />
            <span>{review.entry.label}</span>
          </span>
          {review.entry.text !== null && <span className="diff-muted">{review.entry.text}</span>}
        </div>
      )}
      {summary && (
        <div className="diff-summary">
          <button
            type="button"
            className="diff-summary-head"
            aria-expanded={!folded}
            onClick={() => {
              setFolded(!folded);
            }}
          >
            <Icon name={folded ? "chevron-right" : "chevron-down"} size={12} />
            <Icon name="sparkles" size={13} />
            <span className="diff-summary-title">{summary.title}</span>
            <span className="diff-spacer" />
            <span className="diff-muted">{summary.note}</span>
          </button>
          {!folded && <p className="diff-summary-text">{summary.text}</p>}
        </div>
      )}
      {decisions && (
        <div className="diff-decisions">
          <DecisionsList section={decisions} onChange={onChangeDecision} />
        </div>
      )}
      {review?.report && <Report report={review.report} ticket={ticket} />}
      {review?.foot != null && <p className="diff-entry-foot">{review.foot}</p>}
    </div>
  );
}

export type DiffLoad =
  { state: "loading" } | { state: "loaded"; view: DiffFileView } | { state: "failed"; message: string };

/** Asks for one file's hunks once it comes near the view (or at once without an observer). */
function useDiffWhenVisible(
  ref: RefObject<HTMLElement | null>,
  root: HTMLElement | null,
  wanted: boolean,
  load: () => Promise<DiffFileView>,
): DiffLoad | undefined {
  const [result, setResult] = useState<DiffLoad>();
  const asked = useRef(false);
  useEffect(() => {
    const el = ref.current;
    if (!wanted || asked.current || !el) return;
    const go = () => {
      if (asked.current) return;
      asked.current = true;
      setResult({ state: "loading" });
      load().then(
        (view) => {
          setResult({ state: "loaded", view });
        },
        (error: unknown) => {
          setResult({ state: "failed", message: asRequestError(error).message });
        },
      );
    };
    if (typeof IntersectionObserver === "undefined") {
      go();
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) go();
      },
      { root, rootMargin: "800px 0px" },
    );
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, [ref, root, wanted, load]);
  return result;
}

export function DiffLines({
  load,
  rows,
  sideBySide,
  ctx,
  renameLabel,
}: {
  load: DiffLoad | undefined;
  rows: number;
  sideBySide: boolean;
  ctx: CommentContext | null;
  renameLabel?: string | null;
}) {
  if (load?.state === "loaded") {
    return (
      <>
        {renameLabel != null && (
          <p className="diff-note" data-rename>
            <Icon name="file-symlink" size={16} />
            {renameLabel}
          </p>
        )}
        {load.view.hunks.map((hunk, i) => (
          <Hunk key={i} hunk={hunk} sideBySide={sideBySide} ctx={ctx} />
        ))}
        {load.view.truncated !== null && <p className="diff-note">{load.view.truncated}</p>}
      </>
    );
  }
  if (load?.state === "failed") return <p className="diff-note">{load.message}</p>;
  // Local data arrives at once: room for the lines rather than a spinner.
  return <div style={{ height: rows * LINE_HEIGHT }} />;
}

interface SectionProps {
  ticket: string;
  file: FilesChangeRow;
  open: boolean;
  sideBySide: boolean;
  /** `null`: no line comments (the project Files view). */
  ctx: CommentContext | null;
  root: HTMLElement | null;
  openFileLabel: string;
  loadDiffLabel: string;
  onOpenLines: (id: string) => void;
  onOpenFile: (file: FilesChangeRow) => void;
  register: (id: string, el: HTMLElement | null) => void;
  /** The project Files view's All changes: a file folds to its header line, a click on it unfolds it. */
  folded?: boolean;
  onFold?: (id: string) => void;
}

/** One file of All changes: its header (path, counts, Open file) and its lines, asked for as it comes near. */
export const ChangeSection = memo(function ChangeSection({
  ticket,
  file,
  open,
  sideBySide,
  ctx,
  root,
  openFileLabel,
  loadDiffLabel,
  onOpenLines,
  onOpenFile,
  register,
  folded = false,
  onFold,
}: SectionProps) {
  const ref = useRef<HTMLElement | null>(null);
  const load = useCallback(
    () =>
      request("files.diff", {
        key: ticket,
        path: file.path,
        ...(file.child !== null && { child: file.child }),
      }) as Promise<DiffFileView>,
    [ticket, file.path, file.child],
  );
  const lines = useDiffWhenVisible(ref, root, open && !folded, load);
  let body: ReactNode;
  if (file.binary_label !== null) {
    body = (
      <p className="diff-note">
        <Icon name="image" size={16} />
        {file.binary_label}
      </p>
    );
  } else if (!open) {
    body = (
      <div className="diff-note">
        <span>{file.collapsed}</span>
        <Button
          compact
          variant="secondary"
          label={loadDiffLabel}
          onClick={() => {
            onOpenLines(file.id);
          }}
        />
      </div>
    );
  } else {
    body = (
      <DiffLines
        load={lines}
        rows={file.rows}
        sideBySide={sideBySide}
        ctx={file.commentable ? ctx : null}
        renameLabel={file.rename_label}
      />
    );
  }
  return (
    <section
      ref={(el) => {
        ref.current = el;
        register(file.id, el);
      }}
      className="files-change"
      aria-label={file.label}
    >
      <header className="files-change-head">
        {onFold ? (
          <button
            type="button"
            className="files-fold"
            aria-expanded={!folded}
            onClick={() => {
              onFold(file.id);
            }}
          >
            <Icon name={folded ? "chevron-right" : "chevron-down"} size={12} />
            <span className="files-change-name">
              <Path text={file.label} />
            </span>
          </button>
        ) : (
          <>
            <Icon name="file" size={13} />
            <span className="files-change-name">
              <Path text={file.label} />
            </span>
          </>
        )}
        {file.mark.counts !== null && <span className="files-counts">{file.mark.counts}</span>}
        {file.note !== null && <span className="files-note">{file.note}</span>}
        {file.mark.uncommitted !== null && <span className="files-note">{file.mark.uncommitted}</span>}
        <Tags tags={file.tags} />
        <span className="files-spacer" />
        {file.can_open && (
          <button
            type="button"
            className="files-link"
            onClick={() => {
              onOpenFile(file);
            }}
          >
            {openFileLabel}
          </button>
        )}
      </header>
      {!folded && <div className="files-card">{body}</div>}
    </section>
  );
});

export interface AllChangesProps {
  ticket: string;
  changes: FilesChangesView;
  ctx: CommentContext;
  width: number;
  pane: HTMLElement | null;
  notes?: FilesReviewNotes;
  decisions?: DecisionsSection | null;
  /** The file to scroll to (picked in the Changed tree). */
  scrollTo: string | null;
  onOpenFile: (file: FilesChangeRow) => void;
  onMode: (sideBySide: boolean) => void;
}

/** All changes (FTB-04): the header, the review notes and every changed file's diff in one scroll. */
export function AllChanges({
  ticket,
  changes,
  ctx,
  width,
  pane,
  notes,
  decisions,
  scrollTo,
  onOpenFile,
  onMode,
}: AllChangesProps) {
  const [opened, setOpened] = useState<ReadonlySet<string>>(new Set());
  const sections = useRef(new Map<string, HTMLElement>());
  const register = useCallback((id: string, el: HTMLElement | null) => {
    if (el) sections.current.set(id, el);
    else sections.current.delete(id);
  }, []);
  const onOpenLines = useCallback((id: string) => {
    setOpened((o) => new Set(o).add(id));
  }, []);
  useEffect(() => {
    if (scrollTo !== null) sections.current.get(scrollTo)?.scrollIntoView({ block: "start" });
  }, [scrollTo]);
  const sideBySide = showsSideBySide(changes.side_by_side, width, changes.side_by_side_from);
  return (
    <div className="files-changes">
      <div className="files-main-head">
        <div className="files-main-title">
          <span className="files-title">{changes.title}</span>
          <span className="files-subtitle">
            {changes.summary}
            {changes.note !== null && ` ${changes.note}`}
          </span>
        </div>
        <Segmented
          label={changes.modes.join(" / ")}
          options={changes.modes}
          selected={changes.side_by_side ? 1 : 0}
          onSelect={(i) => {
            onMode(i === 1);
          }}
        />
      </div>
      <Notes
        ticket={ticket}
        notes={notes}
        decisions={decisions}
        onChangeDecision={(item) => {
          openOverlay("change_decision", { id: item.id });
        }}
      />
      {changes.empty ? (
        <div className="diff-empty">
          <div className="diff-empty-head">
            <span>{changes.empty.title}</span>
          </div>
          <p>{changes.empty.text}</p>
        </div>
      ) : (
        changes.files.map((file) => (
          <ChangeSection
            key={file.id}
            ticket={ticket}
            file={file}
            open={showsLines(file, opened)}
            sideBySide={sideBySide}
            ctx={ctx}
            root={pane}
            openFileLabel={changes.open_file}
            loadDiffLabel={changes.load_diff}
            onOpenLines={onOpenLines}
            onOpenFile={onOpenFile}
            register={register}
          />
        ))
      )}
    </div>
  );
}

export interface FileBodyProps {
  ticket: string;
  page: FilesFilePage;
  file: OpenFile;
  ctx: CommentContext;
  sideBySide: boolean;
  onMode: (mode: "file" | "diff") => void;
}

/** A file opened from the tree (FTB-03): its header with File · Diff and New ticket from this file, then its lines. */
export function FileBody({ ticket, page, file, ctx, sideBySide, onMode }: FileBodyProps) {
  const mode = shownMode(file.mode, page);
  const load = useCallback(
    () =>
      request("files.diff", {
        key: ticket,
        path: page.path,
        ...(page.child !== null && { child: page.child }),
      }) as Promise<DiffFileView>,
    [ticket, page.path, page.child],
  );
  // The Diff's lines, for the file they were asked for.
  const [answer, setAnswer] = useState<{ load: typeof load; diff: DiffLoad }>();
  const diff = answer?.load === load ? answer.diff : undefined;
  useEffect(() => {
    if (mode !== "diff") return;
    let current = true;
    load().then(
      (view) => {
        if (current) setAnswer({ load, diff: { state: "loaded", view } });
      },
      (error: unknown) => {
        if (current) setAnswer({ load, diff: { state: "failed", message: asRequestError(error).message } });
      },
    );
    return () => {
      current = false;
    };
  }, [mode, load]);
  // A terminal's `path:line` (d0117 point 8): the line to the middle, marked, once per open.
  const card = useRef<HTMLDivElement>(null);
  const marked = mode === "file" ? (file.line ?? null) : null;
  useScrollToLine(card, marked, file, page);
  return (
    <div className="files-file">
      <div className="files-main-head">
        <div className="files-main-title">
          <span className="files-title files-mono">
            <Path text={page.path} />
          </span>
          <span className="files-subtitle">{page.subtitle}</span>
        </div>
        {page.can_file && page.can_diff && (
          <Segmented
            label={page.modes.join(" / ")}
            options={page.modes}
            selected={mode === "file" ? 0 : 1}
            onSelect={(i) => {
              onMode(i === 0 ? "file" : "diff");
            }}
          />
        )}
        <Button
          compact
          variant="secondary"
          icon="plus"
          label={page.new_ticket}
          onClick={() => {
            openOverlay("new_ticket", { body: page.new_ticket_body });
          }}
        />
      </div>
      {page.writing !== null && (
        <p className="files-writing">
          <Icon name="pencil" size={13} />
          <span>{page.writing}</span>
        </p>
      )}
      <div className="files-card" ref={card}>
        {mode === "file" ? (
          <>
            {chunks(page.lines.length).map(([start, end]) => (
              <div
                key={start}
                className="diff-chunk"
                style={{ containIntrinsicSize: `auto ${String((end - start) * LINE_HEIGHT)}px` }}
              >
                {page.lines.slice(start, end).map((line) => (
                  <CodeLineRow
                    key={line.number}
                    number={line.number}
                    spans={line.spans}
                    marked={line.number === marked}
                  />
                ))}
              </div>
            ))}
            {page.note !== null && <p className="diff-note">{page.note}</p>}
          </>
        ) : (
          <DiffLines load={diff} rows={8} sideBySide={sideBySide} ctx={page.child === null ? ctx : null} />
        )}
      </div>
    </div>
  );
}

/** The open file, asked for by path (and child). */
function FilePane(props: Omit<FileBodyProps, "page">) {
  const { ticket, file } = props;
  const load = useCallback(
    () =>
      request("files.read", {
        key: ticket,
        path: file.path,
        ...(file.child !== null && { child: file.child }),
      }) as Promise<FilesFilePage>,
    [ticket, file.path, file.child],
  );
  const { view: page, error } = useServiceView(load, FILES_EVENTS);
  if (page?.path === file.path) return <FileBody {...props} page={page} />;
  return <p className="files-quiet">{error?.message ?? ""}</p>;
}

/** What the search box shows: its matches as it is typed (the answer to the latest text only). */
function useSearch(ticket: string, query: string): FilesSearchView | null {
  const [found, setFound] = useState<FilesSearchView | null>(null);
  const searching = query.trim() !== "";
  useEffect(() => {
    if (!searching) return;
    let current = true;
    (request("files.search", { key: ticket, query }) as Promise<FilesSearchView>).then((v) => {
      if (current) setFound(v);
    }, ignore);
    return () => {
      current = false;
    };
  }, [ticket, query, searching]);
  // Until the new text's answer comes, the last one's matches stay; an empty box shows the tree.
  return searching ? found : null;
}

export interface FilesTabBodyProps {
  ticket: string;
  view: FilesTabView;
  comments: FilesComments | undefined;
  /** Under the Changed filter: All changes (and the changed tree). */
  changes?: FilesChangesView;
  notes?: FilesReviewNotes;
  decisions?: DecisionsSection | null;
  state: FilesState;
  onState: (change: (s: FilesState) => FilesState) => void;
  /** Reloads All changes after Unified · Split. */
  reload: () => void;
  /** Above the source (the project Files view's back link to the base, FIL-07). */
  head?: ReactNode;
  /** Above the right side (a picked ticket's "Open SHOP-29" and its note in the Files view). */
  note?: ReactNode;
}

/** The tab for a loaded view (also what the markup tests render). */
export function FilesTabBody({
  ticket,
  view,
  comments,
  changes,
  notes,
  decisions,
  state,
  onState,
  reload,
  head,
  note,
}: FilesTabBodyProps) {
  const filter: FilesFilter = shownFilter(state, view.opens_on);
  const ctx = useComments(ticket, comments);
  const [query, setQuery] = useState("");
  // The match Return opens: the first of each new answer until ↑ ↓ move it.
  const [moved, setMoved] = useState<{ found: FilesSearchView | null; index: number }>({
    found: null,
    index: -1,
  });
  const [scrollTo, setScrollTo] = useState<string | null>(null);
  const found = useSearch(ticket, query);
  const search = useRef<HTMLInputElement>(null);
  const [mainRef, width] = useWidth<HTMLDivElement>();
  const [pane, setPane] = useState<HTMLElement | null>(null);
  // ⌥⌘P: asked before the tab showed (it switched to Files) or while it shows.
  const focusAsks = useStore(searchFocus);
  useEffect(() => {
    if (focusAsks <= focusHandled) return;
    focusHandled = focusAsks;
    search.current?.focus();
  }, [focusAsks]);
  const selected = moved.found === found ? moved.index : found && found.rows.length > 0 ? 0 : -1;
  const open = (file: OpenFile) => {
    setQuery("");
    onState((s) => openFile(s, file));
  };
  const onMode = (sideBySide: boolean) => {
    request("files.mode", { side_by_side: sideBySide }).then(reload, ignore);
  };
  const sideBySide = showsSideBySide(
    changes?.side_by_side ?? false,
    width,
    changes?.side_by_side_from ?? Number.POSITIVE_INFINITY,
  );

  let left: ReactNode;
  if (found !== null) {
    left = (
      <SearchResults
        found={found}
        selected={selected}
        onOpen={(path, child) => {
          open({ path, child, mode: null });
        }}
      />
    );
  } else if (filter === "changed") {
    left = (
      <div className="files-list" role="tree" aria-label={view.filters[1]}>
        {(changes?.tree ?? []).map((row) => (
          <TreeRowButton
            key={`${row.kind}:${row.path}`}
            row={row}
            open
            selected={scrollTo === changeId(row)}
            onPress={(r) => {
              if (r.kind !== "folder") setScrollTo(changeId(r));
            }}
          />
        ))}
      </div>
    );
  } else {
    left = <Tree ticket={ticket} view={view} state={state} onState={onState} />;
  }

  let main: ReactNode;
  if (filter === "changed") {
    main = changes ? (
      <AllChanges
        ticket={ticket}
        changes={changes}
        ctx={ctx}
        width={width}
        pane={pane}
        notes={notes}
        decisions={decisions}
        scrollTo={scrollTo}
        onOpenFile={(file) => {
          open({ path: file.path, child: file.child, mode: "file" });
        }}
        onMode={onMode}
      />
    ) : null;
  } else if (state.file) {
    main = (
      <FilePane
        ticket={ticket}
        file={state.file}
        ctx={ctx}
        sideBySide={sideBySide}
        onMode={(mode) => {
          onState((s) => (s.file ? { ...s, file: { ...s.file, mode } } : s));
        }}
      />
    );
  } else {
    main = <p className="files-quiet files-pick">{view.pick}</p>;
  }

  return (
    <div className="files">
      <aside className="files-side">
        {head}
        <div className="files-source">
          <Icon name={view.source.kind === "worktree" ? "folder-git-2" : "git-branch"} size={14} />
          <div className="files-source-text">
            <span className="files-source-title">{view.source.title}</span>
            <span className="files-source-sub">{view.source.subtitle}</span>
          </div>
        </div>
        {view.source.legend.length > 0 && (
          // An epic's key to its tags (FTB-06): "40.1 merged", "40.2 on top · its own worktree".
          <div className="files-legend">
            {view.source.legend.map((t) => (
              <span
                key={t.label}
                className="files-tag files-legend-tag"
                data-writing={t.writing || undefined}
              >
                {t.label}
              </span>
            ))}
          </div>
        )}
        <label className="files-search">
          <Icon name="search" size={13} />
          <input
            {...PLAIN_TEXT}
            ref={search}
            value={query}
            placeholder={view.search.placeholder}
            aria-label={view.search.placeholder}
            onChange={(e) => {
              setQuery(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape" && query !== "") {
                e.preventDefault();
                e.stopPropagation();
                setQuery("");
              } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                setMoved({
                  found,
                  index: steppedRow(selected, found?.rows.length ?? 0, e.key === "ArrowDown" ? 1 : -1),
                });
              } else if (e.key === "Enter") {
                const row = found?.rows[selected];
                if (row) {
                  e.preventDefault();
                  e.stopPropagation();
                  open({ path: row.path, child: row.child, mode: null });
                }
              }
            }}
          />
          {view.search.keys !== null && query === "" && <Kbd keys={view.search.keys} />}
        </label>
        {found === null && (
          <Segmented
            label={view.source.title}
            options={view.filters}
            selected={filter === "all" ? 0 : 1}
            onSelect={(i) => {
              onState((s) => ({ ...s, filter: i === 0 ? "all" : "changed" }));
            }}
          />
        )}
        {left}
      </aside>
      <div
        className="files-main"
        ref={(el) => {
          mainRef.current = el;
          setPane(el);
        }}
      >
        {note}
        {main}
      </div>
    </div>
  );
}

/** The project Files view's picked ticket or epic (FIL-07): its tab, read-only, with the view's back link and note. */
export interface ReadOnlyTab {
  head: ReactNode;
  note: ReactNode;
  /** The source's subtitle, ending "· read-only". */
  subtitle: string;
}

/** The Files tab registered in the sheet: loads the frame, the comments and, under Changed, All changes. Read-only (the
 * Files view's picked source): no comments, review notes or decisions. */
export function FilesTab({ ticket, readOnly }: TicketProps & { readOnly?: ReadOnlyTab }) {
  const [state, onState] = useTabState(ticket);
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => {
    setTick((t) => t + 1);
  }, []);
  const load = useCallback(() => request("files.view", { key: ticket }) as Promise<FilesLoad>, [ticket]);
  const { view: loaded, error } = useServiceView(load, FILES_EVENTS);
  // Kept by identity: the tree asks for its opened folders again only for a new view.
  const subtitle = readOnly?.subtitle;
  const view = useMemo(
    () =>
      subtitle !== undefined && loaded?.view
        ? { ...loaded.view, source: { ...loaded.view.source, subtitle } }
        : loaded?.view,
    [loaded, subtitle],
  );
  const filter = view ? shownFilter(state, view.opens_on) : null;
  // `tick` makes a new load after Unified · Split (the service ignores it).
  const loadChanges = useCallback(
    () =>
      filter === "changed"
        ? (request("files.changes", { key: ticket, tick }) as Promise<FilesChangesView>)
        : Promise.resolve(undefined),
    [ticket, filter, tick],
  );
  const { view: changes } = useServiceView(loadChanges, FILES_EVENTS);
  const writable = readOnly === undefined;
  const loadComments = useCallback(
    () =>
      writable
        ? (request("files.comments", { key: ticket }) as Promise<FilesComments>)
        : Promise.resolve(undefined),
    [ticket, writable],
  );
  const { view: comments } = useServiceView(loadComments, FILES_EVENTS);
  const loadNotes = useCallback(
    () =>
      writable
        ? (request("files.review", { key: ticket }) as Promise<FilesReviewNotes>)
        : Promise.resolve(undefined),
    [ticket, writable],
  );
  const { view: notes } = useServiceView(loadNotes, FILES_EVENTS);
  const loadDecisions = useCallback(
    () =>
      writable
        ? (request("review.decisions", { key: ticket }) as Promise<DecisionsSection | null>)
        : Promise.resolve(null),
    [ticket, writable],
  );
  const { view: decisions } = useServiceView(loadDecisions, FILES_EVENTS);
  if (!view || view.key !== ticket) {
    return <p className="files-quiet files-problem">{loaded?.problem ?? error?.message ?? ""}</p>;
  }
  return (
    <FilesTabBody
      key={ticket}
      ticket={ticket}
      view={view}
      comments={comments?.key === ticket ? comments : undefined}
      changes={changes?.key === ticket ? changes : undefined}
      {...(notes && { notes })}
      decisions={decisions}
      state={state}
      onState={onState}
      reload={reload}
      {...(readOnly && { head: readOnly.head, note: readOnly.note })}
    />
  );
}
