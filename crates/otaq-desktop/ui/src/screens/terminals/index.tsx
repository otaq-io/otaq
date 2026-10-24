// The Terminals drawer (T-0242, T-0331; d0068, d0087): the drawer overlay with the project's Project terminals only,
// and Terminals (⌃`, `g` `t`), which the rail's Terminals runs too: it toggles the drawer, opening it on the last-used terminal (or a new
// one). The Activity popover's "Open Terminals" only opens it (`openTerminals`). New Project agent and Project agents belong to the agent panel (T-0330). What the drawer says is
// Rust's (`src/api/drawer.rs`, `otaq-view`'s `project_terminal.rs`).
import { closeOverlay, openOverlay, overlays } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import { selection } from "../../app/selection";
import { request } from "../../tauri";
import { TerminalsDrawer } from "./Drawer";
import { requestTab } from "./logic";
import "./terminals.css";

/** Opens the drawer on the tab a request answers with (the request picks or opens it on the Rust side). */
export function showDrawerTab(pending: Promise<unknown>): void {
  openOverlay("terminals");
  pending.then(
    (tab) => {
      if (typeof tab === "string") requestTab(tab);
    },
    () => undefined,
  );
}

function drawerOpen(): boolean {
  return overlays.store.get().some((o) => o.id === "terminals");
}

/** Opens the drawer on the last-used terminal (or a new one); an open drawer stays as it is. False with no project. */
export function openTerminals(): boolean {
  const { project } = selection.get();
  if (project === null) return false;
  if (!drawerOpen()) showDrawerTab(request("drawer.last_terminal", { project }));
  return true;
}

export const terminals: Screen = (app) => {
  app.overlays.register("terminals", { Component: TerminalsDrawer });
  app.shortcuts.on("terminals", () => {
    if (selection.get().project !== null && drawerOpen()) {
      closeOverlay("terminals");
      return true;
    }
    return openTerminals();
  });
};
