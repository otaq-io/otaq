// The frame's slots (*02 → Row · Window 1440 × 900 (0086 · V)*), each filled by one screen.
import type { ComponentType } from "react";

export const SLOT_IDS = [
  /** The start of the header row: the inset macOS traffic lights (T-0214). */
  "title_bar_start",
  /** The header (T-0219). The header row is also the title bar. */
  "header",
  /** The header's search and filter field (T-0221), drawn by the header in place of its search entry. */
  "header_search",
  /** The title bar's place after Activity for Pause (PAU-08, T-0457; SHB-01). */
  "header_pause",
  /** The end of the header row: the Windows and Linux caption buttons (T-0214). */
  "title_bar_end",
  /** The project rail (T-0219). */
  "rail",
  /** The banner area across the window under the header, pushing everything under it down (T-0231). */
  "banners",
  /**
   * The Project agent panel between the rail and the main area (SHB-01, SHB-03, T-0330, T-0458). The frame sets its
   * width and keeps it mounted while closed; it reads `shellLayout.agentOpen` if it needs to know.
   */
  "agent_panel",
  /** The board, filling the main area (T-0220, T-0328). */
  "board",
  /** The box for new work at the main area's bottom centre, over the board and the List (UX-22). */
  "new_work",
  /**
   * The ticket sheet over the main area while a ticket is open (SHT-01, T-0464); full page, it fills the main area.
   */
  "sheet",
  /** The toasts, over everything (T-0231). */
  "toasts",
] as const;

export type SlotId = (typeof SLOT_IDS)[number];

export type SlotEntries = Record<SlotId, { Component: ComponentType }>;
