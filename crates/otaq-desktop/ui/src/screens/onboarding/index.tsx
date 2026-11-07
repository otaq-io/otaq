// First run (T-0233): the onboarding overlay, opened at start when no project is stored yet (and on its tools step by
// Check tools), the Add project dialog, and folders dropped on the window. What they say is Rust's
// (`src/api/onboarding.rs`, `onboarding_page.rs`). The usage-data question isn't asked here: it is a quiet banner on
// the board after the first approve (`board/UsageBanner.tsx`).
import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import type { OnboardingStart } from "../../generated/OnboardingStart";
import type { ServiceConnection } from "../../generated/ServiceConnection";
import { pageEvents, watchConnection } from "../../service";
import { request } from "../../tauri";
import { filesDropped } from "../../terminal/drops";
import { AddProjectDialog } from "./AddProjectDialog";
import { folderDropped } from "./drops";
import { OnboardingOverlay } from "./Onboarding";
import "./onboarding.css";

/** Once the service is connected the first time: onboarding opens when no project is stored yet (UI-08). */
export function openOnboardingWhenNeeded(
  ask: typeof request = request,
  watch: (onChange: (c: ServiceConnection) => void) => () => void = watchConnection,
): () => void {
  let asked = false;
  return watch((connection) => {
    if (asked || connection.state !== "connected") return;
    asked = true;
    ask("onboarding.start").then(
      (answer) => {
        if ((answer as OnboardingStart).show) openOverlay("onboarding");
      },
      () => undefined,
    );
  });
}

export const onboarding: Screen = (app) => {
  app.overlays.register("onboarding", { Component: OnboardingOverlay });
  app.overlays.register("add_project", { Component: AddProjectDialog });
  openOnboardingWhenNeeded();
  pageEvents.listen((event) => {
    // On a terminal the paths are pasted into it (d0099); elsewhere the first one is a folder for Add project.
    if (event.kind === "files_dropped") filesDropped(event, folderDropped);
  });
};
