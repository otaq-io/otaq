// The app's own way of carrying out a tool problem's button (`toolProblems.ts`): the service, Settings, Sign in…'s
// terminal dialog and Edit flow.
import { openOverlay } from "../../app/overlays";
import { closeTicket } from "../../app/selection";
import { openFlowBoard } from "../flow_board/logic";
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
  // The editor is the board itself: an open ticket closes so it doesn't stay over it.
  editFlow: (open) => {
    closeTicket();
    openFlowBoard(open);
  },
};

/** A tool problem's button, done in the app. */
export function toolProblemAction(action: ToolProblemAction, place: ToolProblemPlace): void {
  runToolProblemAction(action, place, APP_RUNNER);
}
