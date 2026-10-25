// The ticket sheet's tabs and Overview's sections (SHT-02, T-0464). The sheet (`screens/sheet`) draws the tab bar from
// Rust's `sheet` view and shows the body registered for the shown tab in `sheetTabs`: Overview (`screens/overview`),
// Session (`screens/session`), Files (`screens/files`) and Activity (`screens/activity`).
import type { ComponentType } from "react";

import type { SheetTab } from "../../generated/SheetTab";

/** The sheet's tabs. */
export const SHEET_TABS: readonly SheetTab[] = ["overview", "session", "files", "activity"];

/** Overview's sections, in order (the Messages are Activity's): DEPENDENCIES (DPV-04, `screens/dependencies`). */
export const SHEET_SECTIONS = ["dependencies"] as const;

/** The box under Overview's top while the ticket's PR is open (T-0227, *05 → SHOP-26 · Approved, PR open*). */
export const SHEET_UNDER_TABS = "pr_open" as const;

export type SheetSection = (typeof SHEET_SECTIONS)[number] | typeof SHEET_UNDER_TABS;

/** A tab or section is drawn for the open ticket, by key. */
export interface TicketProps {
  ticket: string;
}

export type SheetTabEntries = Record<SheetTab, { Component: ComponentType<TicketProps> }>;
export type SheetSectionEntries = Record<SheetSection, { Component: ComponentType<TicketProps> }>;
