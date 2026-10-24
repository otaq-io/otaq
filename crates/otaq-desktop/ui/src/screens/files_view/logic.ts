// The project Files view's interaction state (UIT-04): the filter, the folders opened, the file picked with File or
// Diff and Diff's ticket, the source picked in the picker and what is typed in it, per project while the app runs.
// Everything worded or counted comes from Rust (`files_view.rs`).
import { createStore, type Store } from "../../app/store";
import type { FilesFileMode } from "../../generated/FilesFileMode";
import type { FilesViewFile } from "../../generated/FilesViewFile";
import type { FilesViewFilter } from "../../generated/FilesViewFilter";
import type { MenuItem } from "../../generated/MenuItem";
import type { FilesViewPick } from "../../generated/FilesViewPick";
import type { FilesViewPicker } from "../../generated/FilesViewPicker";
import type { FilesViewPickerSection } from "../../generated/FilesViewPickerSection";
import { closeTicket, selection } from "../../app/selection";
import { folders, searchFocus } from "../files/logic";

/** While an agent works, the view asks again this often: commits have no event (FIL-10). */
export const POLL_MS = 5_000;

/** The service events that change the tickets' files: a turn ends, a merge (FIL-10). */
export const PROJECT_FILES_EVENTS = ["card_status_changed", "ticket_changed"] as const;

export interface ViewState {
  filter: FilesViewFilter;
  open: ReadonlySet<string>;
  /** The file picked in the tree or the search. */
  file: string | null;
  /** File or Diff, as chosen for the picked file; `null`: as it opens. */
  mode: FilesFileMode | null;
  /** Diff's ticket, as chosen in "of SHOP-29 ▾"; `null`: the one the file starts on. */
  diffOf: string | null;
  /** A ticket or epic picked in the picker (its key); `null`: the base branch with every ticket's changes. */
  picked: string | null;
  /** One of your own worktrees picked in the picker (its folder; #43): the view shows its tree instead of the base,
   * read-only. */
  worktree: string | null;
  /** The line of the picked file to scroll to and mark (a terminal's path link): a new object each time it is asked
   * for, so asking for the same line again scrolls again. */
  line: { line: number } | null;
}

export const FIRST: ViewState = {
  filter: "all",
  open: new Set(),
  file: null,
  mode: null,
  diffOf: null,
  picked: null,
  worktree: null,
  line: null,
};

/** A file picked where it shows (the tree, All changes' Open file): as it opens, on its first ticket. */
export function withFile(state: ViewState, path: string | null): ViewState {
  return state.file === path ? state : { ...state, file: path, mode: null, diffOf: null, line: null };
}

/** A filter chosen: Changed with no file shows All changes (FIL-08), so choosing it drops the picked file. */
export function withFilter(state: ViewState, filter: FilesViewFilter): ViewState {
  return filter === "changed" ? { ...withFile(state, null), filter } : { ...state, filter };
}

/** How the file shows: as chosen, else as it opens; File is off for a file not on the base, Diff for one nobody
 * changes. */
export function shownFileMode(mode: FilesFileMode | null, view: FilesViewFile): FilesFileMode {
  const wanted = mode ?? view.opens_in;
  if (!view.can_file) return "diff";
  if (!view.can_diff) return "file";
  return wanted;
}

/** Diff's ticket: the one chosen while it still changes the file, else the one it starts on (FIL-05). */
export function diffOwner(chosen: string | null, view: FilesViewFile): string | null {
  if (chosen !== null && view.owners.some((o) => o.key === chosen)) return chosen;
  return view.diff_default ?? view.owners[0]?.key ?? null;
}

/** "of SHOP-29 ▾"'s list: "Diff against main", then each ticket with its stage, lines and +/−, the shown one checked. */
export function ownerMenu(view: FilesViewFile, current: string | null): MenuItem[] {
  const item = (fields: Partial<MenuItem> & Pick<MenuItem, "id" | "label">): MenuItem => ({
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: false,
    header: false,
    ...fields,
  });
  return [
    item({ id: "", label: view.diff_title, header: true }),
    ...view.owners.map((o) =>
      item({ id: o.key, label: o.menu_label, hint: o.menu_hint, checked: o.key === current }),
    ),
  ];
}

/** Each project's view while the app runs, so leaving the view and coming back keeps it. */
export const viewStates: Store<ReadonlyMap<string, ViewState>> = createStore<ReadonlyMap<string, ViewState>>(
  new Map(),
);

/** Picking a file: All, the file selected and the folders above it open. */
export function pickFile(state: ViewState, path: string): ViewState {
  const open = new Set(state.open);
  for (const folder of folders(path)) open.add(folder);
  return { ...withFile(state, path), filter: "all", open };
}

/** A project whose Files view was asked for from outside it (a terminal's path link), counted so asking twice
 * shows it twice; the board shows the view. */
export const filesViewWanted: Store<{ project: string; n: number } | null> = createStore<{
  project: string;
  n: number;
} | null>(null);

/**
 * Opens `path` in `project`'s Files view (d0117 point 8): the base with the file picked, as File at `line` with the
 * line marked when there is one; an open ticket's sheet closes so the view shows.
 */
export function openProjectFile(project: string, path: string, line: number | null): void {
  viewStates.update((m) => {
    const state = pickFile({ ...(m.get(project) ?? FIRST), picked: null, worktree: null }, path);
    return new Map(m).set(project, {
      ...state,
      mode: line === null ? state.mode : "file",
      line: line === null ? null : { line },
    });
  });
  if (selection.get().ticket !== null) closeTicket();
  filesViewWanted.update((w) => ({ project, n: (w?.n ?? 0) + 1 }));
}

/** A row picked in the picker: the base, a ticket or epic, or one of your worktrees. Moving between the base and a
 * worktree starts its tree afresh (their paths differ). */
export function withPick(state: ViewState, pick: FilesViewPick): ViewState {
  const worktree = pick.kind === "worktree" ? pick.path : null;
  const picked = pick.kind === "ticket" ? pick.key : null;
  const fresh =
    worktree === state.worktree
      ? state
      : {
          ...state,
          filter: "all" as const,
          open: new Set<string>(),
          file: null,
          mode: null,
          diffOf: null,
          line: null,
        };
  return { ...fresh, picked, worktree };
}

/** The picker's sections with only the rows whose key or title holds `query` (any case); empty sections go. */
export function pickerSections(picker: FilesViewPicker, query: string): FilesViewPickerSection[] {
  const q = query.trim().toLowerCase();
  if (q === "") return picker.sections;
  return picker.sections
    .map((s) => ({
      ...s,
      rows: s.rows.filter((r) => `${r.key ?? ""} ${r.title}`.toLowerCase().includes(q)),
    }))
    .filter((s) => s.rows.length > 0);
}

/** A request to focus the view's search box (⌥⌘P), counted so the same key twice focuses twice. */
export const viewSearchFocus: Store<number> = createStore(0);

/**
 * ⌥⌘P in the Files view (FIL-11): with a ticket's sheet open the tab's handler takes it (`false`); else the view's
 * search box, or the picked ticket's tab's, is asked for.
 */
export function goToFile(project: string, ticketOpen: boolean): boolean {
  if (ticketOpen) return false;
  if ((viewStates.get().get(project) ?? FIRST).picked !== null) searchFocus.update((n) => n + 1);
  else viewSearchFocus.update((n) => n + 1);
  return true;
}
