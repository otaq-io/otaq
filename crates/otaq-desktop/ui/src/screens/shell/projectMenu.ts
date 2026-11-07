// The project switcher under the project's name in the title bar, and the rail's menu. What they say is Rust's
// (`shell.header`'s `menu`, `shell.rail`'s `project_menu`; `otaq-view`'s `project_menu.rs`); this file only says where
// each choice goes. The name only switches projects or adds one: a project row shows that project, Add project… opens
// the add-project form. The rail's menu (a right-click on a tile, #11) opens the project's Settings: at the top, at its
// Name field (Rename…) or at Remove project.
import { openOverlay } from "../../app/overlays";
import type { OverlayParams } from "../../app/registry/overlays";
import { selectProject } from "../../app/selection";

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

/** Where a choice in the rail's menu opens the project's Settings; `null` for an id it doesn't know. */
export function railMenuTarget(project: string, id: string): OverlayParams["settings"] | null {
  switch (id) {
    case "project_settings":
      return { project };
    case "rename_project":
      return { project, section: "general" };
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
  openSettings: (params: OverlayParams["settings"]) => void = (params) => {
    openOverlay("settings", params);
  },
): void {
  const target = railMenuTarget(project, id);
  if (target) openSettings(target);
}
