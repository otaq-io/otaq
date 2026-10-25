// The Files tab's interaction state (UIT-04): the filter, the folders opened and their rows, the open file and how it
// shows, the search's selected row, which diffs load, the chunks long ones are drawn in, and the comment box being
// edited. Everything worded or decided by the board's rules comes from Rust (`files_tab.rs`, `diff_page.rs`).
import { createStore, type Store } from "../../app/store";
import type { DiffAnchor } from "../../generated/DiffAnchor";
import type { FilesChangeRow } from "../../generated/FilesChangeRow";
import type { FilesComments } from "../../generated/FilesComments";
import type { FilesFileMode } from "../../generated/FilesFileMode";
import type { FilesFilter } from "../../generated/FilesFilter";
import type { FilesPieceGroup } from "../../generated/FilesPieceGroup";
import type { FilesThread } from "../../generated/FilesThread";
import type { FilesTreeRow } from "../../generated/FilesTreeRow";

/** One diff line's height (the frame's 20 px rows); placeholders and chunk sizes are counted in it. */
export const LINE_HEIGHT = 20;
/** Long files are drawn in chunks of this many rows; the webview skips laying out the chunks out of view. */
export const CHUNK_ROWS = 120;

/** A path cut after each "/", where it may wrap. */
export function pathParts(path: string): string[] {
  return path.split(/(?<=\/)/);
}

/** One run of All changes under By file or By piece (d0133): a group's heading (none By file) and its files. */
export interface ChangeRun {
  heading: string | null;
  key: string;
  files: FilesChangeRow[];
}

/**
 * All changes as shown with the piece filter (`piece`, `null` for All) and By file or By piece (d0133, `s30-04`): By
 * file one run of the files the piece changed; By piece a run per piece (a file two pieces changed is under both),
 * then what no piece changed, each run filtered the same way.
 */
export function changeRuns(
  files: readonly FilesChangeRow[],
  groups: readonly FilesPieceGroup[] | null,
  piece: string | null,
  byPiece: boolean,
): ChangeRun[] {
  const keeps = (file: FilesChangeRow) => piece === null || file.pieces.includes(piece);
  if (!byPiece || groups === null) return [{ heading: null, key: "all", files: files.filter(keeps) }];
  return groups
    .filter((g) => piece === null || g.name === piece)
    .map((g) => ({
      heading: g.label,
      key: g.name === "" ? "other" : g.name,
      files: g.files.flatMap((id) => files.filter((f) => f.id === id && keeps(f))),
    }))
    .filter((run) => run.files.length > 0);
}

/** Split when chosen and the pane is wide enough; Unified otherwise (FTB-04). */
export function showsSideBySide(chosen: boolean, width: number, from: number): boolean {
  return chosen && width >= from;
}

/** A set with `item` flipped. */
export function toggled<T>(set: ReadonlySet<T>, item: T): Set<T> {
  const next = new Set(set);
  if (!next.delete(item)) next.add(item);
  return next;
}

/** Whether a file shows its lines: binary files never do; collapsed ones after "Load diff". */
export function showsLines(file: FilesChangeRow, opened: ReadonlySet<string>): boolean {
  return file.binary_label === null && (file.collapsed === null || opened.has(file.id));
}

/** `[start, end)` ranges of `count` rows, `size` at a time. */
export function chunks(count: number, size: number = CHUNK_ROWS): [number, number][] {
  const out: [number, number][] = [];
  for (let start = 0; start < count; start += size) out.push([start, Math.min(count, start + size)]);
  return out;
}

// ---- The tree ----

/**
 * The rows the tree shows: the top folder's, and under each open folder its own rows once loaded (`loaded`, by the
 * folder's path).
 */
export function visibleRows(
  top: readonly FilesTreeRow[],
  open: ReadonlySet<string>,
  loaded: ReadonlyMap<string, readonly FilesTreeRow[]>,
): FilesTreeRow[] {
  const out: FilesTreeRow[] = [];
  const walk = (rows: readonly FilesTreeRow[]) => {
    for (const row of rows) {
      out.push(row);
      if (row.kind !== "folder" || !open.has(row.path)) continue;
      const inner = loaded.get(row.path);
      if (inner) walk(inner);
    }
  };
  walk(top);
  return out;
}

/** The folders a file sits in, outermost first: `src/app/login/page.tsx` → `src`, `src/app`, `src/app/login`. */
export function folders(path: string): string[] {
  const parts = path.split("/").slice(0, -1);
  return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}

/** The row ↑ / ↓ moves to in a list of `count` (`-1`: none yet), stopping at the ends. */
export function steppedRow(current: number, count: number, step: 1 | -1): number {
  if (count === 0) return -1;
  if (current < 0) return step === 1 ? 0 : count - 1;
  return Math.min(Math.max(current + step, 0), count - 1);
}

// ---- What the tab shows ----

/** A file opened from the tree, the search or All changes' Open file; `mode` null: the way Rust opens it. */
export interface OpenFile {
  path: string;
  child: string | null;
  mode: FilesFileMode | null;
  /** The line to scroll to and mark (a terminal's `src/a.ts:42`, d0117 point 8); none from the tree. */
  line?: number | null;
}

/** The tab's own state for a ticket: the filter (null: where Rust opens it), the open file, the folders opened. */
export interface FilesState {
  filter: FilesFilter | null;
  file: OpenFile | null;
  open: ReadonlySet<string>;
}

export const NOTHING_OPEN: FilesState = { filter: null, file: null, open: new Set() };

/** The filter shown: the user's pick, else the one the tab opens on (Changed in your check, SHT-06). */
export function shownFilter(state: FilesState, opensOn: FilesFilter): FilesFilter {
  return state.filter ?? opensOn;
}

/** Opening a file: All files with the file on the right and the folders above it open in the tree. */
export function openFile(state: FilesState, file: OpenFile): FilesState {
  const open = new Set(state.open);
  for (const folder of folders(file.path)) open.add(folder);
  return { filter: "all", file, open };
}

/** The mode a file shows in: the user's pick when the file has it, else the one Rust opens it in. */
export function shownMode(
  picked: FilesFileMode | null,
  page: { opens_in: FilesFileMode; can_file: boolean; can_diff: boolean },
): FilesFileMode {
  if (picked === "file" && page.can_file) return "file";
  if (picked === "diff" && page.can_diff) return "diff";
  return page.opens_in;
}

// ---- Asking for ⌥⌘P's search box and the bar's "being written" ----

/** A request to focus the open tab's search box (⌥⌘P), counted so the same key twice focuses twice. */
export const searchFocus: Store<number> = createStore(0);

/** The ticket whose Files tab has a new comment being written (the bar's "1 being written", `s25-09`). */
export const writing: Store<string | null> = createStore<string | null>(null);

// ---- Line comments ----

/** The threads by their anchor's key. */
export function threadsByAnchor(comments: FilesComments | undefined): Map<string, FilesThread> {
  return new Map((comments?.threads ?? []).map((t) => [t.anchor.key, t]));
}

/** The distinct anchors of a side-by-side row's two halves (a context line is the same anchor on both). */
export function rowAnchors(
  left: DiffAnchor | null | undefined,
  right: DiffAnchor | null | undefined,
): DiffAnchor[] {
  const out: DiffAnchor[] = [];
  for (const a of [left, right]) {
    if (a && !out.some((b) => b.key === a.key)) out.push(a);
  }
  return out;
}

/** The open comment box: a new comment or reply at an anchor, or the edit of a pending comment. The text being typed
 * stays in the box itself, so typing doesn't redraw the diff. */
export interface Composer {
  anchor: DiffAnchor;
  /** The pending comment being edited. */
  editing: string | null;
  /** The thread's first comment, when this is a reply. */
  replyTo: string | null;
  /** The text the box opens with (the comment being edited). */
  initial: string;
  saving: boolean;
  error: string | null;
}

export type ComposerAction =
  | { kind: "open"; anchor: DiffAnchor; replyTo?: string | null }
  | { kind: "edit"; anchor: DiffAnchor; id: string; body: string }
  | { kind: "saving" }
  | { kind: "saved" }
  | { kind: "failed"; message: string }
  | { kind: "cancel" };

export function composerReducer(state: Composer | null, action: ComposerAction): Composer | null {
  switch (action.kind) {
    case "open":
      return {
        anchor: action.anchor,
        editing: null,
        replyTo: action.replyTo ?? null,
        initial: "",
        saving: false,
        error: null,
      };
    case "edit":
      return {
        anchor: action.anchor,
        editing: action.id,
        replyTo: null,
        initial: action.body,
        saving: false,
        error: null,
      };
    case "saving":
      return state && { ...state, saving: true, error: null };
    case "failed":
      return state && { ...state, saving: false, error: action.message };
    case "saved":
    case "cancel":
      return null;
  }
}

/** Whether a new comment is being written (not an edit): the bar counts it (`s25-09`). */
export function writesNew(state: Composer | null): boolean {
  return state !== null && state.editing === null;
}

/** Whether Save can be pressed: some text, and no save on its way (one click, one comment). */
export function canSave(state: Composer | null, text: string): boolean {
  return state !== null && !state.saving && text.trim() !== "";
}

/** The request Save sends: an edit, a reply in the thread, or a new comment on the line. */
export function saveRequest(
  state: Composer,
  text: string,
  ticket: string,
): { method: string; params: Record<string, unknown> } {
  const body = text.trim();
  if (state.editing !== null) return { method: "files.comment_edit", params: { id: state.editing, body } };
  if (state.replyTo !== null) return { method: "files.comment_reply", params: { id: state.replyTo, body } };
  const { file, line, side } = state.anchor;
  return { method: "files.comment_add", params: { key: ticket, file, line, side, body } };
}

/** ⌘↵ (Ctrl+↵ elsewhere) saves the comment; Esc cancels. */
export function composerKey(e: {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
}): "save" | "cancel" | null {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) return "save";
  if (e.key === "Escape") return "cancel";
  return null;
}
