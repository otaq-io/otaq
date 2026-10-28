// Area 15 · Settings (T-0234): the Settings overlay (⌘,), and the global and project sections it draws, one
// registered component per section id (T-0240 registered Validation & Auto-merge; T-0365 Agent tools; T-0395 Keyboard
// shortcuts; T-0410 Agents with one agent's page; T-0422 a project's Flow).
import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import type { GlobalSection, ProjectSection } from "../../app/registry";
import { AboutSection } from "./About";
import { AgentsSection } from "./Agents";
import { AgentToolsSection } from "./AgentTools";
import { FlowSection } from "./Flow";
import { KeyboardSection } from "./Keyboard";
import { PluginsSection, ProjectPluginsSection } from "./Plugins";
import { sectionComponent } from "./Rows";
import { Settings } from "./Settings";
import "./settings.css";

/** The sections this screen draws (all of them since T-0343 added Agents, from their own requests). */
export const GLOBAL: readonly GlobalSection[] = [
  "agent_tools",
  "agents",
  "plugins",
  "agent_limits",
  "usage",
  "notifications",
  "data",
  "appearance",
  "keyboard_shortcuts",
  "tools",
  "about",
];
export const PROJECT: readonly ProjectSection[] = [
  "general",
  "flow",
  "project_notes",
  "answers",
  "commands",
  "ticket_resources",
  "plugins",
  "pr_provider",
  "remove_project",
];

export const settings: Screen = (app) => {
  app.overlays.register("settings", { Component: Settings });
  const globalComponent = (id: GlobalSection) =>
    id === "about"
      ? AboutSection
      : id === "agents"
        ? AgentsSection
        : id === "agent_tools"
          ? AgentToolsSection
          : id === "plugins"
            ? PluginsSection
            : id === "keyboard_shortcuts"
              ? KeyboardSection
              : sectionComponent(id);
  for (const id of GLOBAL) app.settings.global.register(id, { Component: globalComponent(id) });
  for (const id of PROJECT)
    app.settings.project.register(id, {
      Component:
        id === "flow" ? FlowSection : id === "plugins" ? ProjectPluginsSection : sectionComponent(id),
    });
  app.shortcuts.on("settings", () => {
    openOverlay("settings");
  });
};
