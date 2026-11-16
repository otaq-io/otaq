// Where a tool problem's buttons go (§7.2, SHL-04; d0086; T-0367): the sheet's hold callouts, the agent panel's notice
// and the after-restart sheet's Sign in… all send a `ToolProblemAction` (`tool_problems.rs`).
// `toolProblemStep` says what one does in a place, without doing it (the tests read it); `runToolProblemAction` does
// it. Nothing here starts an agent by itself or moves work to another tool: a held agent waits for its own tool, and no
// other agent is offered in its place (no fallback agents).
import type { OverlayParams } from "../../app/registry/overlays";
import type { FlowBoardOpen } from "../flow_board/logic";
import type { AgentToolSignIn } from "../../generated/AgentToolSignIn";
import type { ToolProblemAction } from "../../generated/ToolProblemAction";

/** Where the callout shows: the sheet's ticket, the agent panel's project. */
export interface ToolProblemPlace {
  ticket?: string;
  project?: string;
}

export type ToolProblemStep =
  /** A request to the service. */
  | { kind: "request"; method: string; params: unknown }
  /** The tool's own login in its terminal's dialog (`agent_tools.sign_in`), as Settings → Agents shows it. */
  | { kind: "sign_in"; tool: string }
  | { kind: "settings"; params: NonNullable<OverlayParams["settings"]> }
  /** Edit flow on the project's flow, at the stage that waits (UX-65: a stage's model is the flow's). */
  | { kind: "edit_flow"; open: FlowBoardOpen }
  | { kind: "none" };

/** The project a place is in: its own, or its ticket's (`SHOP-30` is SHOP's). */
function projectOf(place: ToolProblemPlace): string | undefined {
  if (place.project !== undefined) return place.project;
  if (place.ticket === undefined) return undefined;
  const at = place.ticket.lastIndexOf("-");
  return at > 0 ? place.ticket.slice(0, at) : undefined;
}

export function toolProblemStep(action: ToolProblemAction, place: ToolProblemPlace): ToolProblemStep {
  switch (action.kind) {
    case "sign_in":
      return { kind: "sign_in", tool: action.tool };
    case "show_in_agent_tools":
      return { kind: "settings", params: { section: "agent_tools" } };
    case "resume_tool":
      // Resume after reset: the callout's own tool's waiting agents continue (USG-10).
      return { kind: "request", method: "limits.resume_tool", params: { tool: action.tool } };
    // Edit flow…: the stage's model is changed in its project's flow, at that stage when known.
    case "edit_flow": {
      const project = projectOf(place);
      if (project === undefined) return { kind: "none" };
      return {
        kind: "edit_flow",
        open: action.stage === null ? { project } : { project, select: action.stage },
      };
    }
    // Check again: the finder is asked again and the held stage starts if its skill is found.
    case "check_again":
      return place.ticket === undefined
        ? { kind: "none" }
        : { kind: "request", method: "sheet.check_again", params: { key: place.ticket } };
  }
}

/** What carries a step out; the app's own functions, or the tests' stubs. */
export interface ToolProblemRunner {
  request: (method: string, params?: unknown) => Promise<unknown>;
  openSettings: (params: NonNullable<OverlayParams["settings"]>) => void;
  /** Shows Sign in…'s terminal where the user can see it: its own dialog. (Not the Terminals drawer: that lists only
   * the project's terminals, and a sign-in belongs to no project, so the drawer said "Nothing open".) */
  showSignIn: (signIn: AgentToolSignIn) => void;
  /** Opens Edit flow: the board becomes the flow's editor. */
  editFlow: (open: FlowBoardOpen) => void;
}

export function runToolProblemAction(
  action: ToolProblemAction,
  place: ToolProblemPlace,
  run: ToolProblemRunner,
): void {
  const step = toolProblemStep(action, place);
  switch (step.kind) {
    case "request":
      run.request(step.method, step.params).catch(() => undefined);
      return;
    case "sign_in":
      run.request("agent_tools.sign_in", { tool: step.tool }).then(
        (signIn) => {
          run.showSignIn(signIn as AgentToolSignIn);
        },
        () => undefined,
      );
      return;
    case "settings":
      run.openSettings(step.params);
      return;
    case "edit_flow":
      run.editFlow(step.open);
      return;
    case "none":
      return;
  }
}
