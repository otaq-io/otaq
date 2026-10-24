// A file picked in the project Files view (FIL-03, FIL-05, FIL-06, FIL-09; T-0479; `s28-03`, `s28-05`, `s28-07`,
// `s28-08`, `s28-09`, `s28-10`): its path over "on main · 2 tickets change it · read-only", File · Diff ("of SHOP-29 ▾"
// picks Diff's ticket), Copy path and New ticket from this file (a + at 1280 px or with the agent panel open), the
// same-file note, the file as it is on the base with a bar per ticket beside the lines it changes (a huge file folded
// around them), and Changed by on the right (a line of chips above the file when narrow), whose Open on Files opens the
// ticket's sheet on its Files tab with the same file. No line comments: they belong to a ticket's review (d0017). The
// lines, words and colours are Rust's (`files_view.file`, `files_view.diff`); this keeps the moment of interaction.
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { AnchoredMenu, Button, IconButton, Segmented, ToolAvatar, Tooltip } from "../../components";
import type { DiffFileView } from "../../generated/DiffFileView";
import type { FilesFileMode } from "../../generated/FilesFileMode";
import type { FilesViewBar } from "../../generated/FilesViewBar";
import type { FilesViewBarColumn } from "../../generated/FilesViewBarColumn";
import type { FilesViewFile } from "../../generated/FilesViewFile";
import type { FilesViewOwner } from "../../generated/FilesViewOwner";
import type { FilesViewRow } from "../../generated/FilesViewRow";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { paletteInk } from "../../cards/look";
import { type DiffLoad, DiffLines, Path } from "../files/FilesTab";
import { CodeLineRow, useScrollToLine } from "../files/Hunks";
import { chunks, LINE_HEIGHT } from "../files/logic";
import { PROJECT_FILES_EVENTS, diffOwner, ownerMenu, shownFileMode } from "./logic";

/** The bars beside one line: one slot per ticket on the base, in its colour along the line or as a tick between two. */
function Bars({ columns, bars }: { columns: readonly FilesViewBarColumn[]; bars: readonly FilesViewBar[] }) {
  if (columns.length === 0) return null;
  return (
    <span className="files-view-bars" aria-hidden>
      {columns.map((c, i) => (
        <span
          key={c.key}
          className="files-view-bar"
          data-bar={bars[i] ?? "none"}
          style={{ color: `var(--${c.colour})` }}
        />
      ))}
    </span>
  );
}

/** The base's rows: lines with their bars, and the folds of a huge file, drawn in chunks laid out near the view. */
function FileRows({ view, marked }: { view: FilesViewFile; marked: number | null }) {
  return (
    <>
      {chunks(view.rows.length).map(([start, end]) => (
        <div
          key={start}
          className="diff-chunk"
          style={{ containIntrinsicSize: `auto ${String((end - start) * LINE_HEIGHT)}px` }}
        >
          {view.rows.slice(start, end).map((row: FilesViewRow, i) =>
            row.kind === "line" ? (
              <CodeLineRow
                key={row.number}
                number={row.number}
                spans={row.spans}
                bars={<Bars columns={view.bars} bars={row.bars} />}
                marked={row.number === marked}
              />
            ) : (
              <div key={`fold-${String(start + i)}`} className="files-view-fold">
                {row.label}
              </div>
            ),
          )}
        </div>
      ))}
    </>
  );
}

/** Changed by (`s28-03`): each ticket with its tool, key, title, line, branch, lines, +/− and Open on Files. */
export function ChangedBy({ view, onOpen }: { view: FilesViewFile; onOpen: (key: string) => void }) {
  if (view.owners.length === 0) return null;
  return (
    <aside className="files-view-by" aria-label={view.changed_by}>
      <span className="files-view-by-title">{view.changed_by}</span>
      {view.owners.map((o) => (
        <div key={o.key} className="files-view-owner">
          <span className="files-view-edge" style={{ background: `var(--${o.colour})` }} aria-hidden />
          <div className="files-view-owner-body">
            <div className="files-view-owner-head">
              {o.avatar && <ToolAvatar avatar={o.avatar} />}
              <span className="files-view-owner-key">{o.key}</span>
              <span className="files-view-owner-title">{o.title}</span>
            </div>
            {o.status !== null && (
              <span
                className="files-view-owner-status"
                style={o.palette === null ? undefined : { color: paletteInk(o.palette) }}
              >
                <span className="files-view-owner-dot" aria-hidden />
                {o.status}
              </span>
            )}
            <span className="files-view-owner-branch">{o.branch}</span>
            <div className="files-view-owner-foot">
              {o.lines !== null && <span>{o.lines}</span>}
              {o.counts !== null && <span className="files-counts">{o.counts}</span>}
              <span className="files-spacer" />
              <button
                type="button"
                className="files-link"
                onClick={() => {
                  onOpen(o.key);
                }}
              >
                {view.open_on_files}
                <Icon name="arrow-up-right" size={11} />
              </button>
            </div>
          </div>
        </div>
      ))}
      <p className="files-view-by-hint">{view.changed_by_hint}</p>
    </aside>
  );
}

/** At 1280 px or with the agent panel open (`s28-08`): "Changed by" as a line of chips over the file, then Open. */
function ChipsLine({
  view,
  owner,
  onOpen,
}: {
  view: FilesViewFile;
  owner: string | null;
  onOpen: (key: string) => void;
}) {
  if (view.owners.length === 0) return null;
  return (
    <div className="files-view-chips">
      <span className="files-view-chips-title">{view.changed_by}</span>
      {view.owners.map((o: FilesViewOwner) => (
        <Tooltip key={o.key} text={`${o.key} · ${o.title}`} detail={o.status === null ? [] : [o.status]}>
          <button
            type="button"
            className="files-chip files-view-chip"
            onClick={() => {
              onOpen(o.key);
            }}
          >
            <span className="files-chip-dot" style={{ background: `var(--${o.colour})` }} aria-hidden />
            {o.key}
            {o.lines !== null && <span className="files-view-chip-lines">{o.lines}</span>}
          </button>
        </Tooltip>
      ))}
      <span className="files-spacer" />
      {owner !== null && (
        <button
          type="button"
          className="files-link"
          onClick={() => {
            onOpen(owner);
          }}
        >
          {view.open}
        </button>
      )}
    </div>
  );
}

/** "of SHOP-29 ▾": Diff's ticket, a list of the tickets changing the file. */
function OwnerPicker({
  view,
  owner,
  onOwner,
}: {
  view: FilesViewFile;
  owner: string;
  onOwner: (key: string) => void;
}) {
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const shown = view.owners.find((o) => o.key === owner);
  return (
    <>
      <button
        ref={button}
        type="button"
        className="files-view-of"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <span>{view.of}</span>
        <span className="files-chip">
          {shown && (
            <span className="files-chip-dot" style={{ background: `var(--${shown.colour})` }} aria-hidden />
          )}
          {owner}
        </span>
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <AnchoredMenu
          anchor={button}
          label={view.diff_title}
          items={ownerMenu(view, owner)}
          list
          width={280}
          onChoose={onOwner}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") button.current?.focus();
          }}
        />
      )}
    </>
  );
}

export interface FileViewerProps {
  view: FilesViewFile;
  mode: FilesFileMode;
  /** Diff's ticket. */
  owner: string | null;
  /** Diff's lines for `owner`. */
  diff: DiffLoad | undefined;
  /** The line a terminal's path link asked for (d0117 point 8): scrolled to and marked in File. */
  line?: { line: number } | null;
  onMode: (mode: FilesFileMode) => void;
  onOwner: (key: string) => void;
  onNewTicket: () => void;
  onCopyPath: () => void;
  /** Open on Files: the ticket's sheet on its Files tab with this file. */
  onOpen: (key: string) => void;
}

/** The picked file (also what the gallery and the markup tests render), with Changed by beside it. */
export function FileViewer({
  view,
  mode,
  owner,
  diff,
  line = null,
  onMode,
  onOwner,
  onNewTicket,
  onCopyPath,
  onOpen,
}: FileViewerProps) {
  const card = useRef<HTMLDivElement>(null);
  const marked = mode === "file" ? (line?.line ?? null) : null;
  useScrollToLine(card, marked, line, view);
  const shownOwner = view.owners.find((o) => o.key === owner);
  // Diff of a file on the base names whose change it is; a file only in a ticket's changes keeps "not on main".
  const subtitle = mode === "diff" && view.can_file && shownOwner ? shownOwner.diff_subtitle : view.subtitle;
  const narrow =
    mode === "diff" && view.can_file && shownOwner ? shownOwner.diff_subtitle : view.narrow_subtitle;
  const title = (
    <span
      className="files-title files-mono"
      data-struck={view.struck || undefined}
      data-short={view.short_path !== null || undefined}
    >
      {view.short_path ?? <Path text={view.path} />}
    </span>
  );
  let body: ReactNode;
  // A binary file has no lines to compare: its sizes stand for File and Diff alike.
  if (view.binary !== null) {
    body = (
      <div className="files-view-binary">
        <p className="diff-note">
          <Icon name="image" size={16} />
          {view.binary}
        </p>
        {view.no_preview !== null && <p className="files-view-no-preview">{view.no_preview}</p>}
      </div>
    );
  } else if (mode === "diff") {
    body = <DiffLines load={diff} rows={8} sideBySide={false} ctx={null} />;
  } else {
    body = (
      <>
        <FileRows view={view} marked={marked} />
        {view.note !== null && <p className="diff-note">{view.note}</p>}
      </>
    );
  }
  return (
    <>
      <div className="files-main">
        <div className="files-file files-view-file">
          <div className="files-main-head">
            <div className="files-main-title">
              {view.short_path === null ? title : <Tooltip text={view.path}>{title}</Tooltip>}
              <span className="files-subtitle">
                <span className="files-view-wide">{subtitle}</span>
                <span className="files-view-narrow">{narrow}</span>
              </span>
            </div>
            {view.can_file && view.can_diff && (
              <Segmented
                label={view.modes.join(" / ")}
                options={view.modes}
                selected={mode === "file" ? 0 : 1}
                onSelect={(i) => {
                  onMode(i === 0 ? "file" : "diff");
                }}
              />
            )}
            {mode === "diff" && owner !== null && view.owners.length > 1 && (
              <OwnerPicker view={view} owner={owner} onOwner={onOwner} />
            )}
            <span className="files-view-narrow">
              <Tooltip text={view.new_ticket}>
                <IconButton icon="plus" label={view.new_ticket} onClick={onNewTicket} />
              </Tooltip>
            </span>
            <Tooltip text={view.copy_path}>
              <IconButton icon="copy" label={view.copy_path} onClick={onCopyPath} />
            </Tooltip>
            <span className="files-view-wide">
              <Button compact variant="secondary" icon="plus" label={view.new_ticket} onClick={onNewTicket} />
            </span>
          </div>
          <ChipsLine view={view} owner={owner} onOpen={onOpen} />
          {view.same_file.map((note) => (
            <p key={note} className="files-view-note" data-kind="same-file">
              <Icon name="git-compare-arrows" size={14} />
              <span>{note}</span>
            </p>
          ))}
          {view.other_base !== null && (
            <p className="files-view-note" data-kind="info">
              <Icon name="git-branch" size={14} />
              <span>{view.other_base}</span>
            </p>
          )}
          {view.huge !== null && mode === "file" && (
            <p className="files-view-note" data-kind="info">
              <Icon name="info" size={14} />
              <span>{view.huge}</span>
            </p>
          )}
          <div className="files-card" ref={card}>
            {body}
          </div>
        </div>
      </div>
      <ChangedBy view={view} onOpen={onOpen} />
    </>
  );
}

/** Diff's lines for one ticket: the read's own for a file not on the base, else asked for (`files_view.diff`). */
function useOwnerDiff(
  project: string,
  view: FilesViewFile | null,
  owner: string | null,
  wanted: boolean,
): DiffLoad | undefined {
  const included = view !== null && owner === view.diff_of ? view.diff : null;
  const path = view?.path ?? null;
  const key = path === null || owner === null ? null : `${path}\u0000${owner}`;
  const [answer, setAnswer] = useState<{ key: string; diff: DiffLoad }>();
  useEffect(() => {
    if (!wanted || included !== null || key === null || path === null || owner === null) return;
    let current = true;
    (request("files_view.diff", { project, path, ticket: owner }) as Promise<DiffFileView>).then(
      (v) => {
        if (current) setAnswer({ key, diff: { state: "loaded", view: v } });
      },
      (error: unknown) => {
        if (current) setAnswer({ key, diff: { state: "failed", message: asRequestError(error).message } });
      },
    );
    return () => {
      current = false;
    };
    // A new read of the file (an event, the poll while an agent works) asks for its Diff again.
  }, [project, path, owner, key, wanted, included, view]);
  if (included !== null) return { state: "loaded", view: included };
  return answer?.key === key ? answer.diff : undefined;
}

/** Copies a file's path (the menu's and the viewer's Copy path). */
export function copyPath(path: string): void {
  navigator.clipboard.writeText(path).catch(() => undefined);
}

export interface ProjectFilePaneProps {
  project: string;
  path: string;
  /** One of your own worktrees instead of the base (its folder; #43). */
  worktree?: string | null;
  mode: FilesFileMode | null;
  diffOf: string | null;
  /** The line to scroll to and mark (a terminal's path link). */
  line?: { line: number } | null;
  /** While it is read: "on main · checking which tickets change it…" (`s28-09`). */
  checking: string;
  /** Counts up with the view's poll while an agent works: the file is read again (FIL-10). */
  refresh: number;
  onMode: (mode: FilesFileMode) => void;
  onOwner: (key: string) => void;
  onNewTicket: (body: string) => void;
  onOpen: (key: string, mode: FilesFileMode) => void;
}

/** The picked file of the shown project (`files_view.file`), read again on the tickets' events. */
export function ProjectFilePane({
  project,
  path,
  worktree = null,
  mode,
  diffOf,
  line = null,
  checking,
  refresh,
  onMode,
  onOwner,
  onNewTicket,
  onOpen,
}: ProjectFilePaneProps) {
  const load = useCallback(
    () => request("files_view.file", { project, path, worktree }) as Promise<FilesViewFile>,
    // `refresh` asks again while an agent works.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a refetch trigger, not read
    [project, path, worktree, refresh],
  );
  const { view: loaded, error } = useServiceView(load, PROJECT_FILES_EVENTS);
  const view = loaded?.path === path ? loaded : null;
  const shown = view ? shownFileMode(mode, view) : "file";
  const owner = view ? diffOwner(diffOf, view) : null;
  const diff = useOwnerDiff(project, view, owner, shown === "diff");
  if (!view) {
    return (
      <div className="files-main">
        <div className="files-file files-view-file">
          <div className="files-main-head">
            <div className="files-main-title">
              <span className="files-title files-mono">
                <Path text={path} />
              </span>
              <span className="files-subtitle">{error?.message ?? checking}</span>
            </div>
          </div>
        </div>
      </div>
    );
  }
  return (
    <FileViewer
      view={view}
      mode={shown}
      owner={owner}
      diff={diff}
      line={line}
      onMode={onMode}
      onOwner={onOwner}
      onNewTicket={() => {
        onNewTicket(view.new_ticket_body);
      }}
      onCopyPath={() => {
        copyPath(view.path);
      }}
      onOpen={(key) => {
        onOpen(key, shown);
      }}
    />
  );
}
