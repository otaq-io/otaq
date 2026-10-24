// The project Files view (FIL-01…FIL-11; T-0478, T-0479; `s28-03`…`s28-10`): the toolbar's third view. On the left
// the source bar ("main · every ticket’s changes"), which opens the picker (the base branch, epics' branches, every
// ticket with a worktree), the search box (⌥⌘P, Go to file), All · Changed · Needs you · Same file with their counts,
// and the base's tree with every ticket's chips; on the right the picked file with Changed by (`FileViewer.tsx`), or
// under Changed with no file every ticket's changes grouped (`AllChangesView.tsx`). Right-click on a file, the viewer's
// button or ⌘N opens New ticket with its path (FIL-09). A ticket or epic picked in the picker shows its own Files tab,
// read-only, with the back link and "Open SHOP-29". The base shows at once (`base_only`), the marks follow; the view
// asks again on the tickets' events and every 5 s while an agent works, never while it isn't shown. Rows, marks and
// words are Rust's (`files_view.*`, `files_view.rs`); this keeps the moment of interaction only.
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { mainWidth } from "../../app/layout";
import { useShortcut } from "../../app/registry";
import { openTicket, selection } from "../../app/selection";
import { useStore } from "../../app/store";
import { AnchoredMenu, Kbd, Popover, Tooltip, PLAIN_TEXT } from "../../components";
import type { FilesViewChanges } from "../../generated/FilesViewChanges";
import type { FilesSearchView } from "../../generated/FilesSearchView";
import type { FilesTreeRow } from "../../generated/FilesTreeRow";
import type { FilesViewFilter } from "../../generated/FilesViewFilter";
import type { FilesViewLoad } from "../../generated/FilesViewLoad";
import type { FilesViewPage } from "../../generated/FilesViewPage";
import type { FilesViewPickedLoad } from "../../generated/FilesViewPickedLoad";
import type { FilesViewPicker } from "../../generated/FilesViewPicker";
import type { FilesViewPickerRow } from "../../generated/FilesViewPickerRow";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { FilesTab, SearchResults, TreeRowButton, openFileIn, useWidth } from "../files/FilesTab";
import { steppedRow, toggled, visibleRows } from "../files/logic";
import { AllChangesView } from "./AllChangesView";
import { ProjectFilePane, copyPath } from "./FileViewer";
import {
  PROJECT_FILES_EVENTS,
  FIRST,
  POLL_MS,
  type ViewState,
  goToFile,
  pickFile,
  pickerSections,
  viewSearchFocus,
  viewStates,
  withFile,
  withFilter,
  withPick,
} from "./logic";
import "./files_view.css";

const ignore = () => undefined;

/** The last ⌥⌘P the view's search box took. */
let focusHandled = 0;

function useViewState(project: string): [ViewState, (change: (s: ViewState) => ViewState) => void] {
  const all = useStore(viewStates);
  const state = all.get(project) ?? FIRST;
  const update = useCallback(
    (change: (s: ViewState) => ViewState) => {
      viewStates.update((m) => new Map(m).set(project, change(m.get(project) ?? FIRST)));
    },
    [project],
  );
  return [state, update];
}

/** Counts up every [`POLL_MS`] while `on` (an agent works and the view is shown). */
function usePoll(on: boolean): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!on) return;
    const timer = setInterval(() => {
      setTick((t) => t + 1);
    }, POLL_MS);
    return () => {
      clearInterval(timer);
    };
  }, [on]);
  return tick;
}

/** The picker over the source bar (FIL-07, `s28-04`): its sections, filtered by what is typed. */
export function PickerBody({
  picker,
  query,
  onQuery,
  onPick,
}: {
  picker: FilesViewPicker;
  query: string;
  onQuery: (query: string) => void;
  onPick: (row: FilesViewPickerRow) => void;
}) {
  return (
    <div className="files-view-picker">
      <label className="files-search">
        <Icon name="search" size={13} />
        <input
          {...PLAIN_TEXT}
          // eslint-disable-next-line jsx-a11y/no-autofocus -- the picker opens to be typed in
          autoFocus
          value={query}
          placeholder={picker.placeholder}
          aria-label={picker.placeholder}
          onChange={(e) => {
            onQuery(e.target.value);
          }}
        />
      </label>
      {pickerSections(picker, query).map((section) => (
        <div key={section.title} className="files-view-picker-section">
          <span className="files-view-picker-title">{section.title}</span>
          {section.rows.map((row) => (
            <button
              key={row.pick.kind === "base" ? "" : row.pick.kind === "ticket" ? row.pick.key : row.pick.path}
              type="button"
              className="files-view-picker-row"
              onClick={() => {
                onPick(row);
              }}
            >
              {row.key !== null && <span className="files-view-picker-key">{row.key}</span>}
              <span className="files-view-picker-name">{row.title}</span>
              {row.detail !== null && <span className="files-view-picker-detail">{row.detail}</span>}
            </button>
          ))}
        </div>
      ))}
      {picker.no_worktree !== null && <p className="files-view-picker-note">{picker.no_worktree}</p>}
    </div>
  );
}

/** The source bar: "main · every ticket’s changes" over "the base branch · 8 tickets, 11 files"; opens the picker. */
function SourceBar({ project, page }: { project: string; page: FilesViewPage }) {
  const [, update] = useViewState(project);
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const load = useCallback(
    () =>
      open ? (request("files_view.sources", { project }) as Promise<FilesViewPicker>) : Promise.resolve(null),
    [project, open],
  );
  const { view: picker } = useServiceView(load, PROJECT_FILES_EVENTS);
  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
  }, []);
  return (
    <>
      <button
        ref={button}
        type="button"
        className="files-source files-view-source"
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => {
          if (open) close();
          else setOpen(true);
        }}
      >
        <Icon name="git-branch" size={14} />
        <span className="files-source-text">
          <span className="files-source-title">{page.source.title}</span>
          <span className="files-source-sub">{page.source.subtitle}</span>
        </span>
        <Icon name="chevron-down" size={12} />
      </button>
      {open && picker && (
        <Popover label={picker.placeholder} anchor={button} onClose={close}>
          <PickerBody
            picker={picker}
            query={query}
            onQuery={setQuery}
            onPick={(row) => {
              close();
              update((s) => withPick(s, row.pick));
            }}
          />
        </Popover>
      )}
    </>
  );
}

/** All · Changed 11 · Needs you 6 · Same file 1; Needs you says what it counts on hover. */
export function Filters({
  page,
  onFilter,
}: {
  page: FilesViewPage;
  onFilter: (filter: FilesViewFilter) => void;
}) {
  return (
    <div className="segmented files-view-filters" role="radiogroup" aria-label={page.source.title}>
      {page.filters.map((f) => {
        const button = (
          <button
            type="button"
            role="radio"
            className="segment"
            aria-checked={f.filter === page.filter}
            tabIndex={f.filter === page.filter ? 0 : -1}
            onClick={() => {
              onFilter(f.filter);
            }}
          >
            {f.label}
          </button>
        );
        return f.tooltip === null ? (
          <span key={f.filter}>{button}</span>
        ) : (
          <Tooltip key={f.filter} text={f.tooltip} detail={f.detail === null ? [] : [f.detail]}>
            {button}
          </Tooltip>
        );
      })}
    </div>
  );
}

export interface FilesViewBodyProps {
  project: string;
  page: FilesViewPage;
  /** The tree's rows as shown: the opened folders' rows under them. */
  rows: readonly FilesTreeRow[];
  open: ReadonlySet<string>;
  file: string | null;
  /** The search's matches while something is typed. */
  found: FilesSearchView | null;
  query: string;
  onQuery: (query: string) => void;
  searchKey?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  searchRef?: React.RefObject<HTMLInputElement | null>;
  selectedMatch: number;
  onOpenMatch: (path: string) => void;
  onRow: (row: FilesTreeRow) => void;
  /** Right-click on a file: its menu (New ticket from this file, Copy path). */
  onMenu?: (row: FilesTreeRow, at: { left: number; top: number }) => void;
  onFilter: (filter: FilesViewFilter) => void;
  /** The source bar (the gallery draws it without its picker). */
  source?: ReactNode;
  /** The right side: the picked file with Changed by, or All changes; the pick line when absent. */
  pane?: ReactNode;
}

/** The view for a loaded page (also what the gallery and the markup tests render). */
export function FilesViewBody({
  project,
  page,
  rows,
  open,
  file,
  found,
  query,
  onQuery,
  searchKey,
  searchRef,
  selectedMatch,
  onOpenMatch,
  onRow,
  onMenu,
  onFilter,
  source,
  pane,
}: FilesViewBodyProps) {
  let left: ReactNode;
  if (found !== null) {
    left = (
      <SearchResults
        found={found}
        selected={selectedMatch}
        onOpen={(path) => {
          onOpenMatch(path);
        }}
      />
    );
  } else {
    left = (
      <div className="files-list" role="tree" aria-label={page.source.title}>
        {rows.map((row) => (
          <TreeRowButton
            key={`${row.kind}:${row.path}`}
            row={row}
            open={page.filter !== "all" || open.has(row.path)}
            selected={file === row.path}
            onPress={onRow}
            {...(onMenu && row.kind !== "folder" && { onMenu })}
          />
        ))}
      </div>
    );
  }
  let main: ReactNode;
  if (pane !== undefined) {
    main = pane;
  } else if (page.empty !== null) {
    main = (
      <div className="files-main">
        <div className="files-view-empty">
          <Icon name="folder-open" size={18} />
          <p className="files-view-empty-title">{page.empty.title}</p>
          <p className="files-quiet">{page.empty.text}</p>
        </div>
      </div>
    );
  } else {
    main = (
      <div className="files-main">
        <p className="files-quiet files-pick">{page.pick}</p>
      </div>
    );
  }
  return (
    <div className="files files-view" data-project={project}>
      <aside className="files-side">
        {source}
        <label className="files-search">
          <Icon name="search" size={13} />
          <input
            {...PLAIN_TEXT}
            ref={searchRef}
            value={query}
            placeholder={page.search.placeholder}
            aria-label={page.search.placeholder}
            onChange={(e) => {
              onQuery(e.target.value);
            }}
            onKeyDown={searchKey}
          />
          {page.search.keys !== null && query === "" && <Kbd keys={page.search.keys} />}
        </label>
        {found === null && page.filters.length > 0 && <Filters page={page} onFilter={onFilter} />}
        {found === null && page.loading !== null && (
          <p className="files-quiet files-view-loading">{page.loading}</p>
        )}
        {left}
      </aside>
      {main}
    </div>
  );
}

/** All changes for the shown project (`files_view.changes`), read again on the tickets' events and with the view. */
function ProjectChanges({
  project,
  refresh,
  onOpenFile,
}: {
  project: string;
  refresh: number;
  onOpenFile: (path: string) => void;
}) {
  const [tick, setTick] = useState(0);
  const load = useCallback(
    () => request("files_view.changes", { project }) as Promise<FilesViewChanges>,
    // `refresh` (the view's poll) and `tick` (after Unified · Split) ask again.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refetch triggers, not read
    [project, refresh, tick],
  );
  const { view: changes, error } = useServiceView(load, PROJECT_FILES_EVENTS);
  const [paneRef, width] = useWidth<HTMLDivElement>();
  const [pane, setPane] = useState<HTMLElement | null>(null);
  return (
    <div
      className="files-main"
      ref={(el) => {
        paneRef.current = el;
        setPane(el);
      }}
    >
      {changes?.project === project ? (
        <AllChangesView
          changes={changes}
          pane={pane}
          width={width}
          onMode={(sideBySide) => {
            request("files.mode", { side_by_side: sideBySide }).then(() => {
              setTick((t) => t + 1);
            }, ignore);
          }}
          onOpenFile={onOpenFile}
          onOpen={(key) => {
            openTicket(key, "files");
          }}
        />
      ) : (
        <p className="files-quiet files-problem">{error?.message ?? ""}</p>
      )}
    </div>
  );
}

/** New ticket from this file (FIL-09): the form with the path in the description, the title focused. */
function newTicketFrom(path: string): void {
  (request("files_view.new_ticket_body", { path }) as Promise<string>).then((body) => {
    openOverlay("new_ticket", { body });
  }, ignore);
}

/** What the search box shows: its matches as it is typed (the answer to the latest text only). */
function useSearch(project: string, worktree: string | null, query: string): FilesSearchView | null {
  const [found, setFound] = useState<FilesSearchView | null>(null);
  const searching = query.trim() !== "";
  useEffect(() => {
    if (!searching) return;
    let current = true;
    (request("files_view.search", { project, query, worktree }) as Promise<FilesSearchView>).then((v) => {
      if (current) setFound(v);
    }, ignore);
    return () => {
      current = false;
    };
  }, [project, worktree, query, searching]);
  return searching ? found : null;
}

/** The base branch with every ticket's changes on top, or one of your worktrees (`worktree`, its folder; #43): its
 * tree alone, read-only. */
function ProjectSource({ project, worktree }: { project: string; worktree: string | null }) {
  const [state, update] = useViewState(project);
  const loadBase = useCallback(
    () =>
      request("files_view.view", {
        project,
        filter: "all",
        base_only: true,
        worktree,
      }) as Promise<FilesViewLoad>,
    [project, worktree],
  );
  const { view: base } = useServiceView(loadBase, []);
  // Whether the last answer had an agent working: then the view asks again every 5 s.
  const [working, setWorking] = useState(false);
  const tick = usePoll(working);
  const load = useCallback(
    () =>
      (
        request("files_view.view", { project, filter: state.filter, worktree }) as Promise<FilesViewLoad>
      ).then((answer) => {
        setWorking(answer.view?.working ?? false);
        return answer;
      }),
    // `tick` asks again while an agent works (FIL-10).
    // eslint-disable-next-line react-hooks/exhaustive-deps -- tick is a refetch trigger, not read
    [project, worktree, state.filter, tick],
  );
  const { view: full, error } = useServiceView(load, PROJECT_FILES_EVENTS);
  const fresh = full?.view?.project === project && full.view.filter === state.filter ? full.view : null;
  const fallback = state.filter === "all" && base?.view?.project === project ? base.view : null;
  const page = fresh ?? fallback;

  // The opened folders' rows for this page: a new page (the work moved on) asks for them again.
  const [cache, setCache] = useState<{
    page: FilesViewPage | null;
    rows: ReadonlyMap<string, FilesTreeRow[]>;
  }>({
    page: null,
    rows: new Map(),
  });
  const loaded = cache.page === page ? cache.rows : new Map<string, FilesTreeRow[]>();
  const keep = useCallback(
    (path: string, rows: FilesTreeRow[]) => {
      setCache((c) => ({ page, rows: new Map(c.page === page ? c.rows : []).set(path, rows) }));
    },
    [page],
  );
  const askFolder = useCallback(
    (path: string, current: () => boolean = () => true) => {
      (
        request("files_view.tree", { project, path, depth: path.split("/").length, worktree }) as Promise<
          FilesTreeRow[]
        >
      ).then((rows) => {
        if (current()) keep(path, rows);
      }, ignore);
    },
    [project, worktree, keep],
  );
  useEffect(() => {
    if (page?.filter !== "all") return;
    let current = true;
    for (const path of state.open) askFolder(path, () => current);
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per page: a folder opened later loads on its click
  }, [askFolder]);

  const [query, setQuery] = useState("");
  const found = useSearch(project, worktree, query);
  const [moved, setMoved] = useState<{ found: FilesSearchView | null; index: number }>({
    found: null,
    index: -1,
  });
  const selected = moved.found === found ? moved.index : found && found.rows.length > 0 ? 0 : -1;
  const search = useRef<HTMLInputElement>(null);
  const focusAsks = useStore(viewSearchFocus);
  useEffect(() => {
    if (focusAsks <= focusHandled) return;
    focusHandled = focusAsks;
    search.current?.focus();
  }, [focusAsks]);
  const openMatch = (path: string) => {
    setQuery("");
    update((s) => pickFile(s, path));
  };
  // A right-clicked file's menu, at the pointer.
  const [menu, setMenu] = useState<{ path: string; at: { left: number; top: number } } | null>(null);
  // ⌘N with a file picked and no sheet open: New ticket from this file (FIL-09); else the board's New ticket.
  useShortcut(
    "new_ticket",
    useCallback(() => {
      const file = (viewStates.get().get(project) ?? FIRST).file;
      if (file === null || selection.get().ticket !== null) return false;
      newTicketFrom(file);
      return true;
    }, [project]),
  );

  if (!page) return <p className="files-quiet files-problem">{full?.problem ?? error?.message ?? ""}</p>;
  const rows = page.filter === "all" ? visibleRows(page.rows, state.open, loaded) : page.rows;
  let pane: ReactNode | undefined;
  if (state.file !== null) {
    pane = (
      <ProjectFilePane
        key={state.file}
        project={project}
        path={state.file}
        worktree={worktree}
        mode={state.mode}
        diffOf={state.diffOf}
        line={state.line}
        checking={page.checking}
        refresh={tick}
        onMode={(mode) => {
          update((s) => ({ ...s, mode }));
        }}
        onOwner={(diffOf) => {
          update((s) => ({ ...s, diffOf }));
        }}
        onNewTicket={(body) => {
          openOverlay("new_ticket", { body });
        }}
        onOpen={(key) => {
          // Its sheet on Files with the same file; Esc comes back here, the selection kept (FIL-05).
          if (state.file !== null) openFileIn(key, { path: state.file, child: null, mode: null });
        }}
      />
    );
  } else if (page.filter === "changed" && page.empty === null) {
    pane = (
      <ProjectChanges
        project={project}
        refresh={tick}
        onOpenFile={(path) => {
          update((s) => withFile(s, path));
        }}
      />
    );
  }
  return (
    <>
      <FilesViewBody
        project={project}
        page={page}
        rows={rows}
        open={state.open}
        file={state.file}
        found={found}
        query={query}
        onQuery={setQuery}
        searchRef={search}
        searchKey={(e) => {
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
              openMatch(row.path);
            }
          }
        }}
        selectedMatch={selected}
        onOpenMatch={openMatch}
        onRow={(row) => {
          if (row.kind !== "folder") {
            update((s) => withFile(s, row.path));
            return;
          }
          if (page.filter !== "all") return;
          update((s) => ({ ...s, open: toggled(s.open, row.path) }));
          if (!state.open.has(row.path) && !loaded.has(row.path)) askFolder(row.path);
        }}
        onMenu={(row, at) => {
          setMenu({ path: row.path, at });
        }}
        onFilter={(filter) => {
          update((s) => withFilter(s, filter));
        }}
        source={<SourceBar project={project} page={page} />}
        {...(pane !== undefined && { pane })}
      />
      {menu && (
        <AnchoredMenu
          anchor={{ ...menu.at, width: 0, height: 0 }}
          label={menu.path}
          items={page.menu}
          onChoose={(id) => {
            if (id === "new_ticket") newTicketFrom(menu.path);
            else if (id === "copy_path") copyPath(menu.path);
          }}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
    </>
  );
}

/** A ticket or epic picked in the picker: its own Files tab, read-only, with the back link and "Open SHOP-29". */
function PickedSource({ project, picked }: { project: string; picked: string }) {
  const [, update] = useViewState(project);
  const load = useCallback(
    () => request("files_view.picked", { project, key: picked }) as Promise<FilesViewPickedLoad>,
    [project, picked],
  );
  const { view: loaded, error } = useServiceView(load, PROJECT_FILES_EVENTS);
  const back = (label: string) => (
    <button
      type="button"
      className="files-view-back"
      onClick={() => {
        update((s) => ({ ...s, picked: null }));
      }}
    >
      <Icon name="arrow-left" size={12} />
      <span>{label}</span>
    </button>
  );
  const view = loaded?.picked?.key === picked ? loaded.picked : null;
  if (!view) {
    return (
      <div className="files files-view">
        <aside className="files-side">
          {back("")}
          <p className="files-quiet files-problem">{loaded?.problem ?? error?.message ?? ""}</p>
        </aside>
      </div>
    );
  }
  return (
    <div className="files-view-picked">
      <FilesTab
        ticket={picked}
        readOnly={{
          head: back(view.back),
          subtitle: view.tab.source.subtitle,
          note: (
            <div className="files-view-picked-note">
              <span className="files-quiet">{view.note}</span>
              <button
                type="button"
                className="files-view-open"
                onClick={() => {
                  openTicket(picked, "files");
                }}
              >
                <Icon name="external-link" size={12} />
                <span>{view.open}</span>
              </button>
            </div>
          ),
        }}
      />
    </div>
  );
}

/** The main area's width at and under which the view narrows (`s28-08`). */
const NARROW_MAIN = 1216;

/** The view for the shown project (`Board.tsx` draws it in place of the columns on Files). */
export function FilesView({ project }: { project: string }) {
  const [state] = useViewState(project);
  // ⌥⌘P with no ticket open: the view's search box (the picked tab's, when one is picked); with a sheet open the
  // tab's handler takes it (FIL-11).
  useShortcut(
    "go_to_file",
    useCallback(() => goToFile(project, selection.get().ticket !== null), [project]),
  );
  const main = useStore(mainWidth);
  return (
    <div className="files-view-root" data-narrow={(main > 0 && main <= NARROW_MAIN) || undefined}>
      {state.picked === null ? (
        <ProjectSource
          key={`${project}:${state.worktree ?? ""}`}
          project={project}
          worktree={state.worktree}
        />
      ) : (
        <PickedSource key={`${project}:${state.picked}`} project={project} picked={state.picked} />
      )}
    </div>
  );
}
