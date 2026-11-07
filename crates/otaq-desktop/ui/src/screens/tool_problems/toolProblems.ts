// Where a tool problem's buttons go (§7.2, SHL-04; d0086; T-0367): the sheet's hold callouts, the agent panel's notice
// and the after-restart sheet's Sign in… all send a `ToolProblemAction` (`tool_problems.rs`).
// `toolProblemStep` says what one does in a place, without doing it (the tests read it); `runToolProblemAction` does
// it. Nothing here starts an agent by itself or moves work to another tool: a held agent waits for its own tool, and no
// other agent is offered in its place (no fallback agents).
import type { OverlayParams } from "../../app/registry/overlays";
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
  /** The tool's own login in a terminal of the Terminals drawer (`agent_tools.sign_in`). */
  | { kind: "sign_in"; tool: string }
  | { kind: "settings"; params: NonNullable<OverlayParams["settings"]> }
  /** Change agent…'s dialog for the ticket's stage that waits for its tool. */
  | { kind: "change_agent"; ticket: string }
  | { kind: "none" };

export function toolProblemStep(action: ToolProblemAction, place: ToolProblemPlace): ToolProblemStep {
  switch (action.kind) {
    case "sign_in":
      return { kind: "sign_in", tool: action.tool };
    case "show_in_agent_tools":
      return { kind: "settings", params: { section: "agent_tools" } };
    case "resume_tool":
      // Resume after reset: the callout's own tool's waiting agents continue (USG-10).
      return { kind: "request", method: "limits.resume_tool", params: { tool: action.tool } };
    // Open agent… (AGM-03): Settings → Agents on that agent's page.
    case "open_agent":
      return {
        kind: "settings",
        params: action.agent === null ? { section: "agents" } : { section: "agents", agent: action.agent },
      };
    case "change_agent":
      return place.ticket === undefined ? { kind: "none" } : { kind: "change_agent", ticket: place.ticket };
    // Check again: the finder is asked again and the held stage starts if its skill is found.
    case "check_again":
      return place.ticket === undefined
        ? { kind: "none" }
        : { kind: "request", method: "sheet.check_again", params: { key: place.ticket } };
    case "agent_settings":
      return {
        kind: "settings",
        params: action.agent === null ? { section: "agents" } : { section: "agents", agent: action.agent },
      };
  }
}

/** What carries a step out; the app's own functions, or the tests' stubs. */
export interface ToolProblemRunner {
  request: (method: string, params?: unknown) => Promise<unknown>;
  openSettings: (params: NonNullable<OverlayParams["settings"]>) => void;
  /** Opens the drawer on the tab the pending request answers with. */
  showDrawerTab: (pending: Promise<unknown>) => void;
  /** Opens Change agent…'s dialog on the ticket. */
  changeAgent: (ticket: string) => void;
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
      run.showDrawerTab(
        run
          .request("agent_tools.sign_in", { tool: step.tool })
          .then((signIn) => (signIn as AgentToolSignIn).tab),
      );
      return;
    case "settings":
      run.openSettings(step.params);
      return;
    case "change_agent":
      run.changeAgent(step.ticket);
      return;
    case "none":
      return;
  }
}
