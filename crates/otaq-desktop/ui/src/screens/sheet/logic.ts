// The ticket sheet's moment of interaction (SHT-01, SHT-02; T-0464): which of Rust's tabs shows for the selection,
// what a tab key or ⌃Tab selects, the sheet's width now, a tab's dot, and which load of `panel.view` is the one on
// open. The tabs, their keys and the widths are Rust's (`view_models/sheet.rs`, sent as `PanelView.sheet`).
import type { TabIndicator } from "../../components";
import type { PanelViewParams } from "../../generated/PanelViewParams";
import type { SheetTabMark } from "../../generated/SheetTabMark";
import type { SheetContent } from "../../generated/SheetContent";
import type { SheetTab } from "../../generated/SheetTab";
import type { SheetTabView } from "../../generated/SheetTabView";
import type { SheetView } from "../../generated/SheetView";

/**
 * The tab shown for the selection: the one holding the selected content (`run` is Session's), else the tab the
 * sheet opens on (Overview; `selected` is `null` when a ticket was just opened), else the first.
 */
export function shownSheetTab(
  tabs: readonly SheetTabView[],
  selected: SheetContent | null,
  opensOn: SheetTab,
): SheetTabView | undefined {
  return (
    tabs.find((t) => selected !== null && t.contents.includes(selected)) ??
    tabs.find((t) => t.tab === opensOn) ??
    tabs[0]
  );
}

/**
 * What a tab shows: the selected content when it holds it (Session on Run), else the one it lands on (Session: the
 * check while one runs or needs you, else the agent).
 */
export function contentOf(tab: SheetTabView, selected: SheetContent | null): SheetContent {
  return selected !== null && tab.contents.includes(selected) ? selected : tab.lands_on;
}

/** The tab `1`–`4` (or `g o` / `s` / `f` / `a`) selects: the shown one with that number. */
export function numberedSheetTab(tabs: readonly SheetTabView[], n: number): SheetTabView | undefined {
  return tabs.find((t) => t.number === n);
}

/** The tab ⌃Tab / ⌃⇧Tab shows (`step` 1 / -1): the next or previous, round the ends. */
export function steppedSheetTab(
  tabs: readonly SheetTabView[],
  shown: SheetTabView | undefined,
  step: 1 | -1,
): SheetTabView | undefined {
  if (tabs.length === 0) return undefined;
  const at = shown ? tabs.findIndex((t) => t.tab === shown.tab) : -1;
  if (at < 0) return tabs[0];
  return tabs[(at + step + tabs.length) % tabs.length];
}

/**
 * The sheet's width over a main area `mainWidth` px wide (`sheet::width`): 1040 px, 920 px beside the Project agent,
 * never wider than the main area less its margins.
 */
export function sheetWidth(view: SheetView, besideAgent: boolean, mainWidth: number): number {
  const wanted = besideAgent ? view.width_beside_agent : view.width;
  return Math.max(Math.min(wanted, mainWidth - 2 * view.margin), 0);
}

/** A tab's dot, as the Tabs component draws it, else its count ("Tickets 5"). */
export function tabIndicator(
  mark: SheetTabMark | null,
  count: string | null = null,
): TabIndicator | undefined {
  if (mark !== null) return mark;
  return count === null ? undefined : { kind: "note", text: count };
}

/**
 * `panel.view`'s loads for one opening of the sheet on `key`: the first says `opened` (the Files count is taken again
 * then, Q421), the refreshes after it don't. `open` is `false` while the sheet slides out: nothing is taken then.
 */
export function panelViewLoads(key: string, open: boolean): () => PanelViewParams {
  let opened = open;
  return () => {
    const params = { key, opened };
    opened = false;
    return params;
  };
}
