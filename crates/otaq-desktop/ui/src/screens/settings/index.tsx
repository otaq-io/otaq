// Area 15 · Settings (T-0234): the Settings overlay (⌘,), and the app's sections it draws, one registered component
// per section id (T-0365 Agent tools; T-0395 Keyboard shortcuts; T-0410 Agents with one agent's page). App settings
// has eight topics: Agents, Git & pull requests, Limits & usage, Notifications, Appearance, Keyboard, Data and About.
// A project's settings are its own page (`project_settings/`).
import type { ComponentType } from "react";

import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import type { GlobalSection } from "../../app/registry";
import { AboutSection } from "./About";
import { AgentsSection } from "./Agents";
import { AgentToolsSection } from "./AgentTools";
import { GitSection } from "./Git";
import { KeyboardSection } from "./Keyboard";
import { sectionComponent } from "./Rows";
import { Settings } from "./Settings";
import { UsageSection } from "./Usage";
import "./settings.css";

/** The sections this screen draws (all of them since T-0343 added Agents, from their own requests). */
export const GLOBAL: readonly GlobalSection[] = [
  "agent_tools",
  "agents",
  "git",
  "agent_limits",
  "usage",
  "notifications",
  "appearance",
  "keyboard_shortcuts",
  "data",
  "about",
];

/** The sections drawn from their own requests; the rest are rows of the Settings view. */
const OWN: Partial<Record<GlobalSection, ComponentType>> = {
  about: AboutSection,
  agents: AgentsSection,
  agent_tools: AgentToolsSection,
  git: GitSection,
  keyboard_shortcuts: KeyboardSection,
  usage: UsageSection,
};

export const settings: Screen = (app) => {
  app.overlays.register("settings", { Component: Settings });
  const globalComponent = (id: GlobalSection) => OWN[id] ?? sectionComponent(id);
  for (const id of GLOBAL) app.settings.global.register(id, { Component: globalComponent(id) });
  app.shortcuts.on("settings", () => {
    openOverlay("settings");
  });
};
