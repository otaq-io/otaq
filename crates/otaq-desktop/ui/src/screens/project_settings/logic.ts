// Where the project's Settings page is opened from outside it (a merge check's link, the rail's menu, a terminal's
// instructions, Settings → Agents' "Edit in <project>'s settings"): the project is shown, any overlay or ticket over
// it closes, and its view switches to Settings at the section asked for. The board shows the view
// (`board/Board.tsx`); the page scrolls to the section.
import { closeOverlay } from "../../app/overlays";
import { closeTicket, selectProject, selection } from "../../app/selection";
import { createStore, type Store } from "../../app/store";
import type { ProjectSectionId } from "../../generated/ProjectSectionId";

/** A project whose Settings page was asked for, at a section (`null`: the top), counted so asking twice scrolls
 * twice. */
export interface SettingsWanted {
  project: string;
  section: ProjectSectionId | null;
  n: number;
}

export const projectSettingsWanted: Store<SettingsWanted | null> = createStore<SettingsWanted | null>(null);

export interface OpenRoutes {
  select: (project: string) => void;
  closeSettings: () => void;
  closeTicket: () => void;
}

export const OPEN_ROUTES: OpenRoutes = {
  select: (project) => {
    selectProject(project);
  },
  closeSettings: () => {
    closeOverlay("settings");
  },
  closeTicket: () => {
    if (selection.get().ticket !== null) closeTicket();
  },
};

/** Opens `project`'s Settings page in its main area, at `section` when given. */
export function openProjectSettings(
  project: string,
  section: ProjectSectionId | null = null,
  routes: OpenRoutes = OPEN_ROUTES,
): void {
  routes.closeSettings();
  routes.closeTicket();
  routes.select(project);
  projectSettingsWanted.update((w) => ({ project, section, n: (w?.n ?? 0) + 1 }));
}

/** The card the section list marks while the page scrolls: the last one whose top has reached the list's top (a
 * few pixels early), the first before any has; at the very bottom, the last card. */
export function activeCard(tops: readonly number[], scrollTop: number, atBottom: boolean): number {
  if (tops.length === 0) return -1;
  if (atBottom) return tops.length - 1;
  let at = 0;
  tops.forEach((top, i) => {
    if (top <= scrollTop + 24) at = i;
  });
  return at;
}
