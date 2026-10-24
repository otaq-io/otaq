// Where a tool problem's buttons go (§7.2, SHL-04; d0086; T-0367): the sheet's hold callouts, the agent panel's notice, the
// lead's waiting notice and the after-restart sheet's Sign in… all send a `ToolProblemAction` (`tool_problems.rs`).
// `toolProblemStep` says what one does in a place, without doing it (the tests read it); `runToolProblemAction` does
// it. Nothing here starts an agent by itself: Use fallback is the user's click (d0086), New agent on … opens the empty
// agent panel, which starts only when the user sends (d0087).
import type { OverlayParams } from "../../app/registry/overlays";
import type { AgentRole } from "../../generated/AgentRole";
import type { AgentToolSignIn } from "../../generated/AgentToolSignIn";
import type { CardActionKind } from "../../generated/CardActionKind";
import type { SheetCardParams } from "../../generated/SheetCardParams";
import type { ToolProblemAction } from "../../generated/ToolProblemAction";

/** Where the callout shows: the sheet's ticket (or the epic of a lead), the agent panel's project. */
export interface ToolProblemPlace {
  ticket?: string;
  project?: string;
}

export type ToolProblemStep =
  /** A request to the service (Use fallback goes through `sheet.card`, as the card's button does). */
  | { kind: "request"; method: string; params: unknown }
  /** The tool's own login in a terminal of the Terminals drawer (`agent_tools.sign_in`). */
  | { kind: "sign_in"; tool: string }
  | { kind: "settings"; params: NonNullable<OverlayParams["settings"]> }
  /** The agent panel's empty state on `agent`; it starts when the user sends. */
  | { kind: "new_agent"; project: string; agent: string }
  | { kind: "none" };

/** The card action Use fallback is for `role`: the developer's or the validator's (`ticket.use_fallback`). */
function fallbackKind(role: AgentRole): CardActionKind | null {
  switch (role) {
    case "developer":
      return "use_fallback";
    case "validator":
      return "use_validator_fallback";
    case "lead":
    case "project_agent":
      return null;
  }
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
    case "use_fallback_now":
      // Use fallback now: the tool's waiting tickets start on its fallback, by this click only (AGM-03).
      return { kind: "request", method: "limits.use_fallback_now", params: { tool: action.tool } };
    case "use_fallback": {
      const kind = fallbackKind(action.role);
      if (kind === null || place.ticket === undefined) return { kind: "none" };
      const params: SheetCardParams = { key: place.ticket, kind, clear: false };
      return { kind: "request", method: "sheet.card", params };
    }
    // Open agent… (AGM-03): Settings → Agents on that agent's page.
    case "open_agent":
      return {
        kind: "settings",
        params: action.agent === null ? { section: "agents" } : { section: "agents", agent: action.agent },
      };
    case "new_agent":
      return place.project === undefined
        ? { kind: "none" }
        : { kind: "new_agent", project: place.project, agent: action.agent };
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
  newAgentOn: (project: string, agent: string) => void;
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
    case "new_agent":
      run.newAgentOn(step.project, step.agent);
      return;
    case "none":
      return;
  }
}
