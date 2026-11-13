// What the Recovery and Cleanup screen decides on its own (T-0235): when the after-restart sheet opens by itself,
// which Cleanup groups are folded, and the signal the rail's broom re-reads its count on. Every text is Rust's.
import { createStore, type Store } from "../../app/store";
import type { CleanupPage } from "../../generated/CleanupPage";
import type { Notices } from "../../generated/Notices";

/**
 * Whether the notices ask for the summary sheet: a service start the page hasn't opened it for yet (once per
 * start; the banner's Review & resume opens it again).
 */
export function summaryToOpen(notices: Notices | undefined, opened: number | null): number | null {
  const start = notices?.summary_for ?? null;
  return start !== null && start !== opened ? start : null;
}

/** The groups folded when the overlay opens (*Cleanup · 20 items*). */
export function startFolded(page: CleanupPage): Set<string> {
  return new Set(page.groups.filter((g) => g.starts_folded).map((g) => g.group));
}

/** `folded` with `group` folded or unfolded. */
export function toggleFold(folded: ReadonlySet<string>, group: string): Set<string> {
  const next = new Set(folded);
  if (!next.delete(group)) next.add(group);
  return next;
}

/** `keys` in `set` ticked or unticked together (the summary's "+3" row and each row's checkbox). */
export function setUnchecked(
  set: ReadonlySet<string>,
  keys: readonly string[],
  checked: boolean,
): Set<string> {
  const next = new Set(set);
  for (const key of keys) {
    if (checked) next.delete(key);
    else next.add(key);
  }
  return next;
}

/**
 * Whether the after-restart dialog closes once its Resume answered: every row it asked for resumed, so nothing is
 * left for it to show (the rows not ticked wait in Needs you). One that couldn't resume keeps it open on its Retry.
 */
export function closesAfterResume(asked: number, resumed: unknown): boolean {
  return asked > 0 && resumed === asked;
}

/** Bumped after each Cleanup scan, so the rail's broom asks for its count again. */
export const cleanupScans: Store<number> = createStore(0);
