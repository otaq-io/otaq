// The window's layout state (SHB-01, SHL-05, T-0327, T-0458, T-0502): whether the Project agent panel is open or
// expanded, the dragged ticket sheet widths per window size class, and whether the ticket sheet is expanded. All of it
// is page state, so each window keeps its own, for the app's life; only the Project agent panel's dragged width is
// kept in the settings (`layout.agent_panel_width`). The sizes, their bounds and the size classes are Rust's
// (`layout.view`, `panel.rs`); this file keeps only the moment of interaction: the drags and the switches.
import { useEffect, useState } from "react";

import type { LayoutAgentPanelWidthParams } from "../generated/LayoutAgentPanelWidthParams";
import type { LayoutView } from "../generated/LayoutView";
import type { SizeClass } from "../generated/SizeClass";
import type { SizeClassBound } from "../generated/SizeClassBound";
import { request } from "../tauri";
import { createStore, type Store, useStore } from "./store";

/** A width dragged per size class; a class without one shows its default. */
export type ClassDrags = Partial<Record<SizeClass, number>>;

export interface ShellLayout {
  /** The Project agent panel is open between the rail and the main area (⌘L, the rail's button, the Ask bar; d0113). */
  agentOpen: boolean;
  panelWidths: ClassDrags;
  /** Expand (`e`): the ticket sheet takes the main area. */
  panelExpanded: boolean;
  /** Expand in the Project agent panel's header: the open panel lies over the board, full width (T-0502). */
  agentExpanded: boolean;
}

export const INITIAL_LAYOUT: ShellLayout = {
  agentOpen: false,
  panelWidths: {},
  panelExpanded: false,
  agentExpanded: false,
};

export const shellLayout = createStore<ShellLayout>(INITIAL_LAYOUT);

/** Rust's sizes; `undefined` until `layout.view` answered (the agent panel and the ticket sheet wait for it). */
export const layoutView = createStore<LayoutView | undefined>(undefined);

/** The main area's width now, measured by the frame: the panel's widest is this minus what the board keeps. */
export const mainWidth = createStore<number>(0);

/**
 * The Board's count, which the board still sets (`Board.tsx`, Q84). Nothing draws it since the Board | Files tab row
 * went (SHB-01); the board's own toolbar takes it over (T-0460).
 */
export const tabCounts = createStore<Partial<Record<"board", string>>>({});

let loading = false;

/** Asks for `layout.view` once: its sizes are fixed for the life of the app. */
export function loadLayout(): void {
  if (loading || layoutView.get() !== undefined) return;
  loading = true;
  request("layout.view").then(
    (view) => {
      layoutView.set(view as LayoutView);
    },
    (error: unknown) => {
      loading = false;
      console.error("the layout couldn't load", error);
    },
  );
}

/** The class of a window `width` px wide: the first in Rust's table it is narrower than (`layout::size_class`). */
export function sizeClass(width: number, classes: readonly SizeClassBound[]): SizeClass | null {
  const found = classes.find((c) => c.below === null || width < c.below) ?? classes.at(-1);
  return found?.class ?? null;
}

/** `drags` with a width kept for `cls` (`null`: back to the class's default). */
export function withDrag(drags: ClassDrags, cls: SizeClass, width: number | null): ClassDrags {
  return width === null
    ? Object.fromEntries(Object.entries(drags).filter(([c]) => c !== cls))
    : { ...drags, [cls]: width };
}

/** Opens or closes the Project agent panel; the conversation is kept either way (d0113). Closed, it isn't expanded. */
export function setAgentOpen(open: boolean, store: Store<ShellLayout> = shellLayout): void {
  const now = store.get();
  if (now.agentOpen !== open || (!open && now.agentExpanded))
    store.update((l) => ({ ...l, agentOpen: open, agentExpanded: open && l.agentExpanded }));
}

/** Expand and its way back (the header's control, Esc): only an open panel expands. */
export function setAgentExpanded(expanded: boolean, store: Store<ShellLayout> = shellLayout): void {
  const want = expanded && store.get().agentOpen;
  if (store.get().agentExpanded !== want) store.update((l) => ({ ...l, agentExpanded: want }));
}

/**
 * The open panel's width in a window `windowWidth` px wide (`layout::agent_panel_width`): `width` (a drag under way,
 * else the kept one) between the panel's narrowest and what leaves the board its minimum. No window yet (0): no bound.
 */
export function agentPanelWidth(view: LayoutView, windowWidth: number, width = view.agent_panel): number {
  const widest =
    windowWidth > 0 ? Math.max(view.agent_panel_min, windowWidth - view.rail - view.board_min) : Infinity;
  return Math.min(Math.max(width, view.agent_panel_min), widest);
}

/** Keeps the width a drag of the panel's edge ended at (`null`: the double-click, the default); takes Rust's view. */
export function keepAgentPanelWidth(width: number | null): Promise<void> {
  const params: LayoutAgentPanelWidthParams = { width };
  return request("layout.agent_panel_width", params).then((view) => {
    layoutView.set(view as LayoutView);
  });
}

export function toggleExpanded(): void {
  shellLayout.update((l) => ({ ...l, panelExpanded: !l.panelExpanded }));
}

export function useWindowWidth(): number {
  // No document while the markup tests render.
  const [width, setWidth] = useState(() =>
    typeof document === "undefined" ? 0 : document.documentElement.clientWidth,
  );
  useEffect(() => {
    const onResize = () => {
      setWidth(document.documentElement.clientWidth);
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
    };
  }, []);
  return width;
}

/** This window's size class, once `layout.view` arrived. */
export function useSizeClass(): SizeClass | null {
  const view = useStore(layoutView);
  const width = useWindowWidth();
  return view ? sizeClass(width, view.classes) : null;
}
