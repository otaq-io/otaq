// Area 15 · Settings (T-0234): the Settings overlay (⌘,), and the global and project sections it draws, one
// registered component per section id (T-0240 registered Validation & Auto-merge; T-0365 Agent tools; T-0395 Keyboard
// shortcuts; T-0410 Agents with one agent's page; T-0422 a project's Flow). App settings has eight topics: Agents,
// Git & pull requests, Limits & usage, Notifications, Appearance, Keyboard, Data and About.
import type { ComponentType } from "react";

import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import type { GlobalSection, ProjectSection } from "../../app/registry";
import { AboutSection } from "./About";
import { AgentsSection } from "./Agents";
import { AgentToolsSection } from "./AgentTools";
import { FlowSection } from "./Flow";
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
export const PROJECT: readonly ProjectSection[] = [
  "general",
  "flow",
  "project_notes",
  "answers",
  "commands",
  "ticket_resources",
  "pr_provider",
  "remove_project",
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
  for (const id of PROJECT)
    app.settings.project.register(id, { Component: id === "flow" ? FlowSection : sectionComponent(id) });
  app.shortcuts.on("settings", () => {
    openOverlay("settings");
  });
};
