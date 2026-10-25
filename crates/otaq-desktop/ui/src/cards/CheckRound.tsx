// A Check stage with several check agents (d0130; canvas `s29-04` "On the card", `s30-05`): one row per check agent,
// its tool mark, its short name and where it stands (passed, running, held · plan limit, no verdict), then the note
// ("Developer resumes with both reports."). Drawn on the card under its line and in the sheet's header; every word is
// Rust's (`check_agents.rs`).
import { ToolAvatar } from "../components";
import type { CheckAgentStep } from "../generated/CheckAgentStep";
import type { CheckRoundView } from "../generated/CheckRoundView";
import type { IconName } from "../generated/IconName";
import { Icon } from "../theme/Icon";

/** The step's icon and colour, as drawn: a green check, a red cross, an amber pause. */
export function stepLook(step: CheckAgentStep): { icon: IconName; color: string } {
  switch (step) {
    case "passed":
      return { icon: "check", color: "var(--st-approved)" };
    case "failed":
      return { icon: "x", color: "var(--st-error)" };
    case "held":
      return { icon: "pause", color: "var(--st-paused)" };
    case "no_verdict":
      return { icon: "circle-question-mark", color: "var(--st-attn)" };
    case "ended":
      return { icon: "circle-slash", color: "var(--text-muted)" };
    case "running":
      return { icon: "loader-circle", color: "var(--st-working)" };
  }
}

/** The rows (and the note) of a round; the round's own line is the card's status line or the block's heading. */
export function CheckRoundRows({ round }: { round: CheckRoundView }) {
  return (
    <div className="check-round">
      <div className="check-round-agents">
        {round.agents.map((agent) => {
          const look = stepLook(agent.step);
          return (
            <span key={agent.name} className="check-round-agent" data-step={agent.step}>
              {agent.avatar && <ToolAvatar avatar={agent.avatar} />}
              <span className="check-round-name">{agent.name}</span>
              <span className="check-round-icon" style={{ color: look.color }}>
                <Icon name={look.icon} size={12} />
              </span>
              <span className="check-round-state" style={{ color: look.color }}>
                {agent.state}
              </span>
            </span>
          );
        })}
      </div>
      {round.note !== null && <div className="check-round-note">{round.note}</div>}
    </div>
  );
}
