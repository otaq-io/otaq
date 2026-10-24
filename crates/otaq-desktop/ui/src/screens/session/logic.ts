// Run and Shell's interaction rules (UIT-04): what a Run click leads to, which button does what, and what Ask
// the agent's events are for. Every text and state is Rust's (`run_page.rs`, `src/api/run.rs`).
import { openOverlay } from "../../app/overlays";
import type { PageEvent } from "../../generated/PageEvent";
import type { RunButtonView } from "../../generated/RunButtonView";
import type { RunStarted } from "../../generated/RunStarted";
import { request } from "../../tauri";

/** The service events after which the Run tab, the processes and the sheet's pill are asked again. */
export const RUN_EVENTS = ["run_changed", "ticket_changed"] as const;

/** What a button on the Run's line does (Open is the line's link, Edit command its ⋯). */
export type Press =
  { kind: "request"; method: "run.start" | "run.stop" } | { kind: "copy"; text: string } | { kind: "none" };

// No `default`: a new button fails ESLint's switch-exhaustiveness-check until it gets its press.
export function pressOf(button: RunButtonView): Press {
  switch (button.kind) {
    case "run":
    case "retry":
      return { kind: "request", method: "run.start" };
    case "stop":
      return { kind: "request", method: "run.stop" };
    case "copy_lsof":
      return button.copy === null ? { kind: "none" } : { kind: "copy", text: button.copy };
  }
}

/** After a Run click: the project has no run command yet, so its dialog asks for one (RUN-02). */
export function afterStart(started: RunStarted, ticket: string): void {
  if (started.outcome === "needs_command") openOverlay("run_command", { project: started.project, ticket });
}

/** Run (the sheet's button, the tab's Run and Retry): starts the ticket's Run, or asks for the command first. */
export function startRun(ticket: string): Promise<RunStarted | null> {
  return (request("run.start", { key: ticket }) as Promise<RunStarted>).then(
    (started) => {
      afterStart(started, ticket);
      return started;
    },
    // The refusal is a toast (Rust's).
    () => null,
  );
}

/** Ask the agent's progress or answer for `project`, from the page's event stream. */
export function suggestionFor(event: PageEvent, project: string): unknown {
  if (event.kind !== "service" || event.name !== "run_suggestion") return undefined;
  return event.event.project === project ? event.event.update : undefined;
}
