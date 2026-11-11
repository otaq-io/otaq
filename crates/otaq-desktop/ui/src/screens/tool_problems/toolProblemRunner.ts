// The app's own way of carrying out a tool problem's button (`toolProblems.ts`): the service, Settings, Sign in…'s
// terminal dialog, the agent row's picker and the agent panel.
import { openOverlay } from "../../app/overlays";
import type { ToolProblemAction } from "../../generated/ToolProblemAction";
import { request } from "../../tauri";
import { runToolProblemAction, type ToolProblemPlace, type ToolProblemRunner } from "./toolProblems";

export const APP_RUNNER: ToolProblemRunner = {
  request,
  openSettings: (params) => {
    openOverlay("settings", params);
  },
  showSignIn: (signIn) => {
    openOverlay("sign_in", signIn);
  },
  changeAgent: (ticket) => {
    openOverlay("change_agent", { ticket });
  },
};

/** A tool problem's button, done in the app. */
export function toolProblemAction(action: ToolProblemAction, place: ToolProblemPlace): void {
  runToolProblemAction(action, place, APP_RUNNER);
}
