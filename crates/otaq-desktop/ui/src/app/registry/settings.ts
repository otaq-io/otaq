// Settings sections (*15 · Settings*), in nav order: the global page's and each project page's. The Settings screen
// (T-0234) draws the sections the Rust view sends (`settings_page.rs`) that are registered here; each section's
// screen registers its own. Labels, rows and icons come from the Rust settings view; the ids are its
// `GlobalSectionId` / `ProjectSectionId`, and the lists below are checked against them both ways.
import type { ComponentType } from "react";

import type { GlobalSectionId } from "../../generated/GlobalSectionId";
import type { ProjectSectionId } from "../../generated/ProjectSectionId";

/** Every id of the union, once: a missing or unknown id fails the type check. */
type Every<U extends string, L extends readonly U[]> = [Exclude<U, L[number]>] extends [never] ? L : never;
const every =
  <U extends string>() =>
  <L extends readonly U[]>(list: Every<U, L>) =>
    list;

export const GLOBAL_SECTIONS = every<GlobalSectionId>()([
  /** M23: the agent tools (T-0360's view; T-0365 draws it). */
  "agent_tools",
  /** The agents, the default model and the fallbacks, and one agent's page (T-0410). */
  "agents",
  /** Tracker plugins and the marketplaces (the otaq-marketplace README). */
  "plugins",
  "agent_limits",
  "usage",
  "notifications",
  "data",
  "appearance",
  /** M29: custom shortcuts and the single-letter switch (T-0395). */
  "keyboard_shortcuts",
  "tools",
  /** The last one: Settings → About (d0071). */
  "about",
] as const);

export const PROJECT_SECTIONS = every<ProjectSectionId>()([
  /** Name, key, repository, base branch. */
  "general",
  /** The project's flow: the stage table and its actions (T-0422, FLW-07). */
  "flow",
  /** The notes every agent in the project gets (AGM-02). */
  "project_notes",
  /** "Let finished tickets answer questions". */
  "answers",
  /** Setup and run commands, Open URL, env files. */
  "commands",
  /** Create, Remove and Template refresh commands, schema paths (M19-d). */
  "ticket_resources",
  /** The tracker plugins the project uses (the otaq-marketplace README). */
  "plugins",
  "pr_provider",
  "remove_project",
] as const);

export type GlobalSection = GlobalSectionId;
export type ProjectSection = ProjectSectionId;

export type GlobalSectionEntries = Record<GlobalSection, { Component: ComponentType }>;
/** A project section is drawn for one project, by key. */
export type ProjectSectionEntries = Record<ProjectSection, { Component: ComponentType<{ project: string }> }>;
