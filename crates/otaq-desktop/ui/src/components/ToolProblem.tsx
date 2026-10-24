// A tool problem's callout or notice (§7.2, SHL-04, AGT-05; d0086, d0077; *05 → Column 3 · several tools (M23)*,
// *18 → Row · Pane: empty, hidden, tool problems, menu (0087)*, *08 → Row · Epic lead in the epic form and panel*):
// "Model unavailable", "Codex isn't available · logged out", "Claude Code limit reached · resets 3:45 pm", the line
// under it and its buttons (Use fallback, Change agent, Sign in…, New agent on Sonnet, Resume after reset). Amber for a
// tool that isn't available or a model no longer listed, neutral for a limit; never turquoise (d0077). The edge look is
// the sheet's (a 2 px edge above the rows); the band look is the agent panel's and the lead's (a soft band with the avatar).
// Everything it says is `tool_problems.rs`'s; what a button does is the caller's (`onAction`).
import type { ToolProblemAction } from "../generated/ToolProblemAction";
import type { ToolProblemCallout as CalloutView } from "../generated/ToolProblemCallout";
import { Button } from "./Button";
import { ToolAvatar } from "./ToolAvatar";
import { Tooltip } from "./Tooltip";
import "./ToolProblem.css";

export type ToolProblemLook = "edge" | "band";

export interface ToolProblemCalloutProps {
  callout: CalloutView;
  look: ToolProblemLook;
  onAction: (action: ToolProblemAction) => void;
}

export function ToolProblemCallout({ callout, look, onAction }: ToolProblemCalloutProps) {
  const heading = <span className="tool-problem-heading">{callout.heading}</span>;
  return (
    <div className="tool-problem" data-look={look} data-tone={callout.tone} role="status">
      {look === "band" && callout.avatar !== null && <ToolAvatar avatar={callout.avatar} />}
      <div className="tool-problem-body">
        <p className="tool-problem-lines">
          {look === "edge" && callout.avatar !== null && <ToolAvatar avatar={callout.avatar} />}
          {/* The model on hover (Model unavailable): "gpt-5.4". */}
          {callout.model === null ? heading : <Tooltip text={callout.model}>{heading}</Tooltip>}
        </p>
        {callout.text !== null && <p className="tool-problem-text">{callout.text}</p>}
        {callout.buttons.length > 0 && (
          <div className="tool-problem-buttons">
            {callout.buttons.map((button) => (
              <Button
                key={button.label}
                compact
                variant={button.primary ? "primary" : "secondary"}
                label={button.label}
                onClick={() => {
                  onAction(button.action);
                }}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
