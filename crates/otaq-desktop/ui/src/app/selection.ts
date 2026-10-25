// The selection screens share (T-0212): the project on screen, the ticket open in the sheet and what it shows, and
// the terminal that has the keyboard. One store for the whole page; each screen reads it with `useSelection()` and
// changes it with the functions below (or `selection.update`).
import type { SheetContent } from "../generated/SheetContent";
import { createStore, type Store, useStore } from "./store";

export interface Selection {
  /** The project on screen, by key (`SHOP`); `null` before a project is added. */
  project: string | null;
  /** The ticket open in the sheet, by key (`SHOP-22`); `null` while it is closed. */
  ticket: string | null;
  /**
   * What the sheet shows: a tab, or a side of Session's switch (`run`: Session on Run; `check`: the check's agent;
   * SHT-02, SHT-04). `null` for the tab it opens on (Overview, or Files in your check).
   */
  tab: SheetContent | null;
  /** The session whose terminal has the keyboard (SES-10's focused-terminal rule); `null` when none has. */
  focusedTerminal: string | null;
}

export const NOTHING_SELECTED: Selection = {
  project: null,
  ticket: null,
  tab: null,
  focusedTerminal: null,
};

export function createSelection(initial: Selection = NOTHING_SELECTED): Store<Selection> {
  return createStore(initial);
}

/** The page's selection. */
export const selection = createSelection();

export function useSelection(): Selection {
  return useStore(selection);
}

/**
 * Asked before the sheet moves away from what it shows (another ticket or tab, closing it, another project): `next` is
 * the selection the move makes, `go` makes it. Returning `true` holds the move (the guard asks first, and calls `go`
 * once the user agrees); `false` lets it happen. Overview's changed description holds it (T-0465, SHT-03).
 */
export type LeaveGuard = (next: Selection, go: () => void) => boolean;

const guards = new WeakMap<Store<Selection>, LeaveGuard>();

/** Sets the store's guard (one at a time: the shown Overview's). Returns the removal. */
export function guardLeaving(guard: LeaveGuard, store: Store<Selection> = selection): () => void {
  guards.set(store, guard);
  return () => {
    if (guards.get(store) === guard) guards.delete(store);
  };
}

/** Makes the move `change`, unless the guard holds it. */
function move(store: Store<Selection>, change: (current: Selection) => Selection): void {
  const now = store.get();
  const next = change(now);
  const go = () => {
    store.update(change);
  };
  if (next !== now && guards.get(store)?.(next, go) === true) return;
  go();
}

/** What each project showed when the user left it, per store: switching back brings it back (T-0219). */
const left = new WeakMap<Store<Selection>, Map<string, Pick<Selection, "ticket" | "tab">>>();

/** Shows another project as the user left it: its open ticket and tab; a project not shown before starts closed. */
export function selectProject(project: string, store: Store<Selection> = selection): void {
  const kept = left.get(store) ?? new Map<string, Pick<Selection, "ticket" | "tab">>();
  left.set(store, kept);
  move(store, (s) => {
    if (s.project === project) return s;
    if (s.project !== null) kept.set(s.project, { ticket: s.ticket, tab: s.tab });
    const back = kept.get(project) ?? { ticket: null, tab: null };
    return { ...s, project, ...back, focusedTerminal: null };
  });
}

/**
 * Opens `ticket` in the sheet, showing `tab` if given, else on the tab it opens on (Overview, SHT-02); the ticket
 * already open keeps what it shows.
 */
export function openTicket(ticket: string, tab?: SheetContent, store: Store<Selection> = selection): void {
  move(store, (s) => ({ ...s, ticket, tab: tab ?? (s.ticket === ticket ? s.tab : null) }));
}

/** Moves the open sheet to the previous or next ticket (j / k), keeping the tab it shows. */
export function stepTicket(ticket: string, store: Store<Selection> = selection): void {
  move(store, (s) => ({ ...s, ticket }));
}

/** Closes the sheet. */
export function closeTicket(store: Store<Selection> = selection): void {
  move(store, (s) => ({ ...s, ticket: null, focusedTerminal: null }));
}

export function showTab(tab: SheetContent, store: Store<Selection> = selection): void {
  move(store, (s) => ({ ...s, tab }));
}

/** Records which terminal has the keyboard (`null`: none). */
export function focusTerminal(session: string | null, store: Store<Selection> = selection): void {
  store.update((s) => (s.focusedTerminal === session ? s : { ...s, focusedTerminal: session }));
}

/**
 * A request to give a ticket's terminal the keyboard once Session shows it (a notification click, T-0231). Which side is the selection's (`agent`, `check`).
 */
export interface TerminalFocusRequest {
  ticket: string;
  /** Counts up with each request, so asking twice for the same ticket is two requests. */
  seq: number;
}

/** The newest terminal focus request; the shown terminal (Session's, T-0466) focuses when it is for its ticket. */
export const terminalFocusRequests: Store<TerminalFocusRequest | null> =
  createStore<TerminalFocusRequest | null>(null);

export function requestTerminalFocus(
  ticket: string,
  store: Store<TerminalFocusRequest | null> = terminalFocusRequests,
): void {
  store.update((last) => ({ ticket, seq: (last?.seq ?? 0) + 1 }));
}

/** A request to show a ticket's whole form (⋯ Edit ticket…, T-0222), in its Overview. */
export interface TicketEditRequest {
  ticket: string;
  /** Counts up with each request, so asking twice for the same ticket is two requests. */
  seq: number;
}

/** The newest edit request; Overview (T-0465) opens its form when it is for its ticket. */
export const ticketEditRequests: Store<TicketEditRequest | null> = createStore<TicketEditRequest | null>(
  null,
);

/** Shows the Overview of `ticket` and asks it for its form. */
export function requestTicketEdit(
  ticket: string,
  store: Store<TicketEditRequest | null> = ticketEditRequests,
  shown: Store<Selection> = selection,
): void {
  openTicket(ticket, "overview", shown);
  store.update((last) => ({ ticket, seq: (last?.seq ?? 0) + 1 }));
}
