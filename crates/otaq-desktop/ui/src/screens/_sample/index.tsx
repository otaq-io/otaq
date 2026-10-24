// The template screen (T-0212): one folder, one exported `Screen` that registers its parts, and one line in the
// screens index. It is not in the index; `registry.test.ts` adds it there to show that nothing else changes.
import { toggleOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";

export const sample: Screen = (app) => {
  // Every slot, overlay, sheet tab and section and settings section has its screen since T-0235, so the sample
  // registers only a shortcut handler; a screen registers its own components the same way (`app.overlays.register`).
  app.shortcuts.on("switch_tab", ({ number }) => {
    // Another tab's number: leave it to the handler registered before (the sheet's).
    if (number !== 6) return false;
    toggleOverlay("processes");
    return true;
  });
};
