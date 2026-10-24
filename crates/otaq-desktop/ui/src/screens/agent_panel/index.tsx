// The Project agent panel (T-0330, T-0458; SHL-04, SHB-03, SHB-04, d0087, d0113): the `agent_panel` slot, Open / close the
// Project agent (⌘L: opens and focuses it, or closes it from inside) and New Project agent conversation (⇧⌘L: the empty
// panel; nothing starts). What it says is Rust's (`src/api/agent_panel.rs`, `otaq-view`'s `agent_panel.rs`).
import type { Screen } from "../../app/registry";
import { focusAgentPanel, inPanel, newProjectAgent, AGENT_PANEL_ROUTES } from "./logic";
import { AgentPanel } from "./AgentPanel";
import "./agent_panel.css";

export const agentPanel: Screen = (app) => {
  app.slots.register("agent_panel", { Component: AgentPanel });
  app.shortcuts.on("focus_agent_panel", () =>
    focusAgentPanel(AGENT_PANEL_ROUTES, typeof document !== "undefined" && inPanel(document.activeElement)),
  );
  app.shortcuts.on("new_project_agent", () => newProjectAgent());
};
