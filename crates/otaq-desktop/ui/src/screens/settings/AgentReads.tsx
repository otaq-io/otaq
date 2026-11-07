// What this agent reads (T-0440; AGR-01…AGR-03; d0108 rule 12; *15 → Agents (update AG §8)* `s15-58` / `s15-59`
// "What this agent reads", "Another project: no notes, two stages"; the Also has: line and the role parts in the same
// style): one agent's full text for a project, read-only, in Settings' agent place. Everything comes from the view
// (`settings.agent_reads`); this draws it and sends the picks and the links.
import { IconButton, Select } from "../../components";
import type { AgentReadsPart } from "../../generated/AgentReadsPart";
import type { AgentReadsTarget } from "../../generated/AgentReadsTarget";
import type { AgentReadsView } from "../../generated/AgentReadsView";
import { Icon } from "../../theme/Icon";
import { flowSoftBreaks } from "./reads";

/** What the view asks for. */
export interface ReadsActions {
  onProject: (project: string) => void;
  onLink: (target: AgentReadsTarget) => void;
}

function Part({ part, onLink }: { part: AgentReadsPart; onLink: (target: AgentReadsTarget) => void }) {
  return (
    <div className="settings-reads-part">
      <span className="settings-reads-part-head">
        <span className="settings-reads-heading">{part.heading}</span>
        {part.link && (
          <button
            type="button"
            className="settings-link settings-reads-link"
            onClick={() => {
              if (part.link) onLink(part.link.target);
            }}
          >
            {part.link.label}
          </button>
        )}
      </span>
      <div
        className="settings-reads-text"
        data-mono={part.mono || undefined}
        data-empty={part.text === null || undefined}
      >
        {/* A SKILL.md's paragraphs flow, so its long lines break once, where the box ends. */}
        {part.text !== null ? (part.mono ? flowSoftBreaks(part.text) : part.text) : part.empty}
      </div>
    </div>
  );
}

/** The view's body: the Project picker, the parts, Also has:, and the last line. */
export function AgentReadsBody({ view, actions }: { view: AgentReadsView; actions: ReadsActions }) {
  return (
    <section className="settings-section settings-reads" data-section="agent_reads" aria-label={view.title}>
      {view.empty !== null ? (
        <p className="settings-reads-empty">{view.empty}</p>
      ) : (
        <span className="settings-reads-picker">
          <span className="settings-reads-picker-label">{view.project_label}</span>
          <span className="settings-reads-select">
            <Select
              label={view.project_label}
              value={view.project_name ?? ""}
              items={view.projects}
              onChange={actions.onProject}
            />
          </span>
          <span className="settings-reads-hint">{view.project_hint}</span>
        </span>
      )}
      {view.parts.map((part) => (
        <Part key={part.heading} part={part} onLink={actions.onLink} />
      ))}
      {view.no_stage !== null && <p className="settings-reads-line">{view.no_stage}</p>}
      {view.also && (
        <p className="settings-reads-also">
          <span>{view.also.label}</span>
          {view.also.skills.map((skill, i) => (
            <span key={skill.name} className="settings-reads-other">
              {i > 0 && <span className="settings-agents-dot">·</span>}
              {skill.link ? (
                <button
                  type="button"
                  className="settings-link settings-reads-name"
                  onClick={() => {
                    if (skill.link) actions.onLink(skill.link);
                  }}
                >
                  {skill.name}
                </button>
              ) : (
                <code className="settings-reads-name">{skill.name}</code>
              )}
              {skill.note !== null && <span className="settings-reads-note">{skill.note}</span>}
            </span>
          ))}
        </p>
      )}
      <p className="settings-reads-not-shown">
        <Icon name="info" size={12} />
        <span>{view.not_shown}</span>
      </p>
    </section>
  );
}

/** Its heading in the Settings header: ← What this agent reads, "<agent> · read-only" under it. */
export function AgentReadsHeading({ view, onBack }: { view: AgentReadsView; onBack: () => void }) {
  return (
    <span className="settings-agent-heading">
      <span className="settings-agent-heading-line">
        <IconButton icon="arrow-left" label={view.back_label} onClick={onBack} />
        <h2>{view.title}</h2>
      </span>
      <span className="settings-agents-came">{view.subtitle}</span>
    </span>
  );
}
