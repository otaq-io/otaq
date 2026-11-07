// The flow editor's parts Edit flow on the board (`flow_board/`) draws a stage's details with (T-0471, T-0472; FED-04…
// FED-06; d0115): the Skill field (Pick a skill…, Ask the agent to write it, which only asks for the user's words,
// Q441; being written with Stop; written with Open and Undo) and a problem's fix line, with FLW-04's controls for an
// import clash or a model to pick (Q426). What they say is Rust's (`flow_board.view`, `otaq-view`'s `flow_editor.rs`).
import { useEffect, useId, useRef, useState } from "react";

import "./flow_editor.css";

import { AnchoredMenu, Button, Radio, Select, Tooltip } from "../../components";
import { CodeText } from "../../components/CodeText";
import type { FlowEditorChoices } from "../../generated/FlowEditorChoices";
import type { FlowEditorClashPick } from "../../generated/FlowEditorClashPick";
import type { FlowEditorFix } from "../../generated/FlowEditorFix";
import type { FlowEditorSkillField } from "../../generated/FlowEditorSkillField";
import type { FlowEditorStageEdit } from "../../generated/FlowEditorStageEdit";
import type { IconName } from "../../generated/IconName";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { optionItems } from "../settings/Agents";
import { ModelPicker } from "../settings/ModelPicker";
import { type SkillButton, typedItem, withClash, withModel } from "./logic";

const ignore = () => undefined;

/** What the Skill field's and a fix line's clicks do: the board's editor's calls. */
export interface EditorActions {
  edit: (edit: FlowEditorStageEdit) => void;
  /** Ask the agent to write it: `prefill` starts the user's words; nothing is sent. */
  write: (stage: string, prefill: string) => void;
  /** The Skill field's other buttons on a stage: Stop, Undo, Open. */
  skill: (stage: string, button: SkillButton) => void;
  /** A pick for a problem (Q426): the picks so far, changed. */
  fix: (change: (choices: FlowEditorChoices) => FlowEditorChoices) => void;
}

/**
 * The Skill field (FED-06): the stage's skill (click it to pick another), with "You don't have the skill `<name>`"
 * under it in amber when nobody has it; empty with Pick a skill… (the user's own skills, then Otaq's, searchable, and
 * "Use `<typed text>`" last while searching) and Ask the agent to write it; "Being written by Noyan…" with Stop; or the
 * skill the agent wrote, tinted, with Open and Undo, and where it was saved under it.
 */
export function SkillField({
  field,
  stage,
  actions,
  focus = false,
}: {
  field: FlowEditorSkillField;
  stage: string;
  actions: EditorActions;
  /** Opened on this field ("use your own Open PR skill" → the Delivery stage's Skill): its first button gets the
   * keyboard and the field scrolls into view. */
  focus?: boolean;
}) {
  const [picking, setPicking] = useState(false);
  const anchor = useRef<HTMLSpanElement>(null);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!focus) return;
    const button = box.current?.querySelector("button");
    button?.focus();
    box.current?.scrollIntoView({ block: "nearest" });
  }, [focus]);
  const state = field.state;
  const pick = () => {
    setPicking(true);
  };
  let body: React.ReactNode;
  switch (state.state) {
    case "set":
      body = (
        <>
          <span ref={anchor} className="fe-skill-anchor">
            <button type="button" className="fe-value fe-skill fe-skill-set" onClick={pick}>
              <Icon name="scroll-text" size={13} />
              {state.skill}
              <Icon name="chevron-down" size={12} />
            </button>
          </span>
          {state.warning !== null && (
            <div className="fe-fix" data-tone="amber">
              <Icon name="triangle-alert" size={14} />
              <span className="fe-fix-text">
                {/* One span, so the name flows in the sentence (the column would put it on a line of its own). */}
                <span>
                  <CodeText text={state.warning} />
                </span>
              </span>
            </div>
          )}
        </>
      );
      break;
    case "empty":
      body = (
        <div className="fe-skill-empty">
          <p className="fe-skill-text">{state.text}</p>
          <div className="fe-skill-buttons">
            <span ref={anchor} className="fe-skill-anchor">
              <Button compact variant="secondary" icon="list" label={state.pick} onClick={pick} />
            </span>
            <Button
              compact
              variant="secondary"
              icon="sparkles"
              label={state.ask}
              onClick={() => {
                actions.write(stage, state.prefill);
              }}
            />
          </div>
          <span className="fe-hint">
            <Icon name="info" size={13} />
            {state.hint}
          </span>
        </div>
      );
      break;
    case "writing":
      body = (
        <div className="fe-skill-box" data-writing data-loading>
          <Icon name="loader-circle" size={14} />
          <span className="fe-skill-writing">{state.text}</span>
          <span className="fe-spacer" />
          <button
            type="button"
            className="fe-link"
            onClick={() => {
              actions.skill(stage, "stop_writing");
            }}
          >
            {state.stop}
          </button>
        </div>
      );
      break;
    case "written":
      body = (
        <div className="fe-skill-box" data-written>
          <Tooltip text={state.why}>
            <span className="fe-skill">
              <Icon name="scroll-text" size={13} />
              {state.skill}
            </span>
          </Tooltip>
          <span className="fe-mark-label fe-skill-note">
            <Icon name="sparkles" size={11} />
            {state.note}
          </span>
          <span className="fe-spacer" />
          {state.open !== null && (
            <button
              type="button"
              className="fe-link"
              onClick={() => {
                actions.skill(stage, "open_skill");
              }}
            >
              {state.open}
            </button>
          )}
          <button
            type="button"
            className="fe-link"
            onClick={() => {
              actions.skill(stage, "undo_skill");
            }}
          >
            {state.undo}
          </button>
        </div>
      );
      if (state.saved !== null) {
        body = (
          <>
            {body}
            <span className="fe-hint">
              <Icon name="info" size={13} />
              {/* One span, so the sentence flows with its folder inline (the hint's flex row would make columns). */}
              <span>
                <CodeText text={state.saved} />
              </span>
            </span>
          </>
        );
      }
      break;
  }
  return (
    <div ref={box} className="fe-field" data-field="skill">
      <span className="fe-label">{field.label}</span>
      {body}
      {picking && (
        <AnchoredMenu
          anchor={anchor}
          align="start"
          label={field.picker.title}
          items={field.picker.items}
          list
          search={field.picker.search}
          typed={(text) => typedItem(field.picker.use_typed, text)}
          onChoose={(skill) => {
            setPicking(false);
            if (skill !== "") actions.edit({ edit: "skill", stage, skill });
          }}
          onClose={() => {
            setPicking(false);
          }}
        />
      )}
    </div>
  );
}

/**
 * A problem's fix, or an import clash (Q426; FLW-04's controls): Keep mine · Take theirs · Add as; the model menu (the
 * default model or a connected tool, then that tool's models).
 */
export function FixLine({ fix, actions }: { fix: FlowEditorFix; actions: EditorActions }) {
  const name = useId();
  const control = fix.control;
  let below: React.ReactNode = null;
  let tone: "red" | "amber" | "ok" = fix.fixed ? "ok" : "red";
  let icon: IconName = fix.fixed ? "circle-check" : "circle-x";
  switch (control.kind) {
    case "clash":
      tone = "amber";
      icon = "git-compare";
      below = (
        <div className="fe-fix-clash" role="radiogroup" aria-label={fix.text}>
          {control.options.map((o) => (
            <Radio
              key={o.id}
              name={name}
              checked={o.selected}
              label={o.label}
              onChange={(checked) => {
                if (checked) actions.fix((c) => withClash(c, control.agent, o.id as FlowEditorClashPick));
              }}
            />
          ))}
        </div>
      );
      break;
    case "model": {
      const model = control.fix;
      const chosen = model.tools.find((t) => t.selected);
      below = (
        <span className="fe-fix-model">
          <Select
            label={model.tool_label}
            value={chosen?.label ?? model.tool_label}
            items={optionItems(model.tools)}
            onChange={(id) => {
              actions.fix((c) => withModel(c, model.agent, id));
            }}
          />
          {model.picker !== null && chosen !== undefined && (
            <ModelPicker
              label={model.model_label}
              picker={model.picker}
              onPick={(picked) => {
                actions.fix((c) => withModel(c, model.agent, chosen.id, picked));
              }}
              onRetry={(t) => {
                request("settings.agents_models_retry", { tool: t }).catch(ignore);
              }}
            />
          )}
        </span>
      );
      break;
    }
  }
  return (
    <div className="fe-fix" data-tone={tone}>
      <Icon name={icon} size={14} />
      <div className="fe-fix-text">
        <div className="fe-fix-line">
          <span>{fix.text}</span>
        </div>
        {below}
      </div>
    </div>
  );
}
