// "Before you" on a human check after a Check stage with several check agents (d0130; canvas `s30-05`): the round
// ("Before you · Validate, round 1 of 3", "2 check agents · both passed"), then a block per check agent with its tool
// mark, name, verdict and model, its comment and Open terminal, which shows that agent's side of Session. Its words
// are Rust's (`validation_report.rs`, `BeforeYouView`).
import { showTab } from "../../app/selection";
import { Button, ToolAvatar } from "../../components";
import { stepLook } from "../../cards";
import type { BeforeYouView } from "../../generated/BeforeYouView";
import { Icon } from "../../theme/Icon";
import { shownCheckAgent } from "../session/terminalLogic";

export function BeforeYou({ view }: { view: BeforeYouView }) {
  return (
    <section className="before-you" aria-label={view.heading}>
      <span className="editor-section-name">{view.heading.toUpperCase()}</span>
      <span className="before-you-summary">{view.summary}</span>
      {view.agents.map((agent) => {
        const look = stepLook(agent.step);
        return (
          <div key={agent.name} className="before-you-agent">
            <div className="before-you-head">
              {agent.avatar && <ToolAvatar avatar={agent.avatar} />}
              <span className="before-you-name">{agent.name}</span>
              <span className="before-you-verdict" style={{ color: look.color }}>
                <Icon name={look.icon} size={12} />
                {agent.verdict}
              </span>
              {agent.model !== null && <span className="before-you-model">{agent.model}</span>}
              <span className="before-you-spacer" />
              <Button
                variant="secondary"
                compact
                label={agent.open_terminal}
                onClick={() => {
                  shownCheckAgent.set(agent.name);
                  showTab("check");
                }}
              />
            </div>
            {agent.comment !== null && <p className="before-you-comment">{agent.comment}</p>}
          </div>
        );
      })}
    </section>
  );
}
