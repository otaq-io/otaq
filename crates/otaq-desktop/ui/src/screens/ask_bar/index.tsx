// The Ask bar (T-0458; SHB-03, d0113): the `ask_bar` slot over the main area. Its keys (⌘L, ⇧⌘L) are the panel's
// (`screens/agent_panel`); what it says is Rust's (`src/api/ask_bar.rs`, `otaq-view`'s `ask_bar.rs`).
import type { Screen } from "../../app/registry";
import { AskBar } from "./AskBar";
import "./ask_bar.css";

export const askBar: Screen = (app) => {
  app.slots.register("ask_bar", { Component: AskBar });
};
