// The registries screens plug into (T-0212). A screen is a function that registers its parts; the screens index
// (`ui/src/screens/index.ts`) lists every screen, one line each, and the page runs them all once at start. See the
// crate README, "How a screen plugs in".
import { createContext, useContext, useEffect } from "react";

import { OVERLAY_IDS, type OverlayEntries } from "./overlays";
import {
  SHEET_SECTIONS,
  SHEET_UNDER_TABS,
  type SheetSectionEntries,
  SHEET_TABS,
  type SheetTabEntries,
} from "./sheet";
import { createRegistry, type Registry } from "./registry";
import {
  GLOBAL_SECTIONS,
  type GlobalSectionEntries,
  PROJECT_SECTIONS,
  type ProjectSectionEntries,
} from "./settings";
import { createShortcuts, type KeyActionName, type ShortcutHandler, type Shortcuts } from "./shortcuts";
import { SLOT_IDS, type SlotEntries } from "./slots";

export interface Registries {
  slots: Registry<SlotEntries>;
  overlays: Registry<OverlayEntries>;
  sheetTabs: Registry<SheetTabEntries>;
  sheetSections: Registry<SheetSectionEntries>;
  settings: { global: Registry<GlobalSectionEntries>; project: Registry<ProjectSectionEntries> };
  shortcuts: Shortcuts;
}

/** A screen: registers its slot, overlays, tabs, sections and shortcut handlers. */
export type Screen = (app: Registries) => void;

export function createRegistries(): Registries {
  return {
    slots: createRegistry("slots", SLOT_IDS),
    overlays: createRegistry("overlays", OVERLAY_IDS),
    sheetTabs: createRegistry("sheet tabs", SHEET_TABS),
    sheetSections: createRegistry("sheet sections", [...SHEET_SECTIONS, SHEET_UNDER_TABS]),
    settings: {
      global: createRegistry("global settings sections", GLOBAL_SECTIONS),
      project: createRegistry("project settings sections", PROJECT_SECTIONS),
    },
    shortcuts: createShortcuts(),
  };
}

/** Fresh registries filled by `screens` (the screens index's exports). */
export function registerScreens(screens: Record<string, Screen>): Registries {
  const app = createRegistries();
  for (const screen of Object.values(screens)) screen(app);
  return app;
}

/** What is registered where: for tests and for checking a screen's footprint. */
export function registered(app: Registries): Record<string, string[]> {
  return {
    slots: app.slots.ids(),
    overlays: app.overlays.ids(),
    sheetTabs: app.sheetTabs.ids(),
    sheetSections: app.sheetSections.ids(),
    globalSettings: app.settings.global.ids(),
    projectSettings: app.settings.project.ids(),
    shortcuts: app.shortcuts.names().toSorted(),
  };
}

export const RegistriesContext = createContext<Registries | null>(null);

/** The page's registries (the frame provides them). */
export function useRegistries(): Registries {
  const app = useContext(RegistriesContext);
  if (app === null) throw new Error("useRegistries outside the frame's RegistriesContext");
  return app;
}

/**
 * A shortcut handler that lives while the component is mounted, for keys that act on it (Find in a focused
 * terminal). `handler` should keep its identity between renders (`useCallback`).
 */
export function useShortcut<N extends KeyActionName>(name: N, handler: ShortcutHandler<N>): void {
  const app = useRegistries();
  useEffect(() => app.shortcuts.on(name, handler), [app, name, handler]);
}

export type { Registry } from "./registry";
export type { OverlayId, OverlayParams, OverlayProps, Confirmation } from "./overlays";
export type { SheetSection, TicketProps } from "./sheet";
export type { GlobalSection, ProjectSection } from "./settings";
export type { ActionNamed, KeyActionName, ShortcutHandler } from "./shortcuts";
export type { SlotId } from "./slots";
