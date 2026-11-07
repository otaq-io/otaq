// The project's Settings page (the view switch's Settings, in the project's main area): the cards it draws, one
// registered component per section id. The board shows the page (`board/Board.tsx`); `openProjectSettings` opens it
// from elsewhere.
import type { ProjectSection, Screen } from "../../app/registry";
import { FlowSection } from "../settings/Flow";
import { sectionComponent } from "../settings/Rows";
import "./project_settings.css";

/** The cards, in the page's order. */
export const PROJECT: readonly ProjectSection[] = [
  "flow",
  "repository",
  "run",
  "workspace",
  "project_notes",
  "remove_project",
];

export const projectSettings: Screen = (app) => {
  for (const id of PROJECT)
    app.settings.project.register(id, { Component: id === "flow" ? FlowSection : sectionComponent(id) });
};
