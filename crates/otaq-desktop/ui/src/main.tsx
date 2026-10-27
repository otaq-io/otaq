import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { registerScreens } from "./app/registry";
import type { MoveAppView } from "./generated/MoveAppView";
import * as screens from "./screens";
import { MoveApp } from "./screens/move_app/MoveApp";
import { request } from "./tauri";
import "./styles.css";

const root = document.getElementById("root");
if (root) {
  // A macOS release outside an Applications folder shows only Move to Applications (T-0579); otherwise the app.
  const start = (move: MoveAppView | null) => {
    createRoot(root).render(
      <StrictMode>
        {move ? <MoveApp initial={move} /> : <App registries={registerScreens(screens)} />}
      </StrictMode>,
    );
  };
  request("move_app.view")
    .then((view) => {
      start(view as MoveAppView | null);
    })
    .catch(() => {
      start(null);
    });
}
