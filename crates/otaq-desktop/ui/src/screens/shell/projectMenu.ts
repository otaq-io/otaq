// The project switcher under the project's name in the title bar, and the rail's menu. What they say is Rust's
// (`shell.header`'s `menu`, `shell.rail`'s `project_menu`; `otaq-view`'s `project_menu.rs`); this file only says where
// each choice goes. The name only switches projects or adds one: a project row shows that project, Add project… opens
// the add-project form. The rail's menu (a right-click on a tile, #11) opens the project's Settings page: at the top,
// at its Repository card with the Name field (Rename…) or at Remove project.
import { openOverlay } from "../../app/overlays";
import { selectProject } from "../../app/selection";
import type { ProjectSectionId } from "../../generated/ProjectSectionId";
import { openProjectSettings } from "../project_settings/logic";

export interface SwitcherRoutes {
  select: (project: string) => void;
  addProject: () => void;
}

export const SWITCHER_ROUTES: SwitcherRoutes = {
  select: selectProject,
  addProject: () => {
    openOverlay("add_project");
  },
};

/** A pick in the switcher: a project's key, or `null` for Add project…. */
export function switcherChoice(key: string | null, routes: SwitcherRoutes = SWITCHER_ROUTES): void {
  if (key === null) routes.addProject();
  else routes.select(key);
}

/** Where a choice in the rail's menu opens the project's Settings page; `null` for an id it doesn't know. */
export function railMenuTarget(
  project: string,
  id: string,
): { project: string; section: ProjectSectionId | null } | null {
  switch (id) {
    case "project_settings":
      return { project, section: null };
    case "rename_project":
      return { project, section: "repository" };
    case "remove_project":
      return { project, section: "remove_project" };
    default:
      return null;
  }
}

/** The rail's menu choice for `project`. */
export function railMenuChoice(
  project: string,
  id: string,
  openSettings: (target: { project: string; section: ProjectSectionId | null }) => void = (target) => {
    openProjectSettings(target.project, target.section);
  },
): void {
  const target = railMenuTarget(project, id);
  if (target) openSettings(target);
}
