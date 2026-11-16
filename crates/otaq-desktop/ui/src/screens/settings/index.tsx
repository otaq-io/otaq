// Area 15 · Settings (T-0234): the Settings overlay (⌘,), and the app's sections it draws, one registered component
// per section id (T-0365 Tools; UX-70 Noyan; T-0395 Keyboard shortcuts). App settings has nine topics: Tools, Noyan,
// Git & pull requests, Limits & usage, Notifications, Appearance, Keyboard, Data and About.
// A project's settings are its own page (`project_settings/`).
import type { ComponentType } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps, Screen } from "../../app/registry";
import type { GlobalSection } from "../../app/registry";
import { AboutSection } from "./About";
import { AgentToolsSection, SignInDialog } from "./AgentTools";
import { GitSection } from "./Git";
import { KeyboardSection } from "./Keyboard";
import { NoyanSettingsSection } from "./NoyanSettings";
import { sectionComponent } from "./Rows";
import { Settings } from "./Settings";
import { UsageSection } from "./Usage";
import "./settings.css";

/** The sections this screen draws (all of them, some from their own requests). */
export const GLOBAL: readonly GlobalSection[] = [
  "tools",
  "noyan",
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
  tools: AgentToolsSection,
  noyan: NoyanSettingsSection,
  git: GitSection,
  keyboard_shortcuts: KeyboardSection,
  usage: UsageSection,
};

/** A tool problem's Sign in… (the sheet, the agent panel, a notice): the same terminal dialog as Settings → Tools'. */
export function SignInOverlay({ params, close }: OverlayProps<"sign_in">) {
  return <SignInDialog signIn={params} onClose={close} />;
}

export const settings: Screen = (app) => {
  app.overlays.register("settings", { Component: Settings });
  app.overlays.register("sign_in", { Component: SignInOverlay });
  const globalComponent = (id: GlobalSection) => OWN[id] ?? sectionComponent(id);
  for (const id of GLOBAL) app.settings.global.register(id, { Component: globalComponent(id) });
  app.shortcuts.on("settings", () => {
    openOverlay("settings");
  });
};
