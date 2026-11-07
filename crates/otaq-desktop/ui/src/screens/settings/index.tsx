// Area 15 · Settings (T-0234): the Settings overlay (⌘,), and the app's sections it draws, one registered component
// per section id (T-0365 Agent tools; T-0395 Keyboard shortcuts; T-0410 Agents with one agent's page). A project's
// settings are its own page (`project_settings/`).
import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import type { GlobalSection } from "../../app/registry";
import { AboutSection } from "./About";
import { AgentsSection } from "./Agents";
import { AgentToolsSection } from "./AgentTools";
import { KeyboardSection } from "./Keyboard";
import { sectionComponent } from "./Rows";
import { Settings } from "./Settings";
import "./settings.css";

/** The sections this screen draws (all of them since T-0343 added Agents, from their own requests). */
export const GLOBAL: readonly GlobalSection[] = [
  "agent_tools",
  "agents",
  "agent_limits",
  "usage",
  "notifications",
  "data",
  "appearance",
  "keyboard_shortcuts",
  "tools",
  "about",
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
          : id === "keyboard_shortcuts"
            ? KeyboardSection
            : sectionComponent(id);
  for (const id of GLOBAL) app.settings.global.register(id, { Component: globalComponent(id) });
  app.shortcuts.on("settings", () => {
    openOverlay("settings");
  });
};
