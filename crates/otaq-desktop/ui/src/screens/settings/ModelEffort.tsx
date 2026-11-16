// Model and Effort: the one way a model is picked (UX-65, UX-68, UX-75; canvas A-2…A-4). A stage's Who does it in Edit
// flow draws it; Done's model, Settings → Tools' "Noyan starts on" and the Noyan panel's header draw the same field.
// The field shows the tool's avatar and "Codex · gpt-5.5"; its menu has "As in your other flows" on top (one click sets
// model and effort), then each tool set up on this Mac with `default` first, then the tools that aren't set up with
// Set up…. A model whose tool isn't set up is marked in amber with Set up <Tool> and Pick another model (A-4). Effort
// is the tool's levels, hidden for a tool with none. Every word is `ModelEffortView`'s (`otaq-view`'s
// `model_effort.rs`); this keeps only whether the menu is open.
import { useRef, useState } from "react";

import { Button, Segmented, ToolAvatar } from "../../components";
import type { ModelEffortView } from "../../generated/ModelEffortView";
import { Icon } from "../../theme/Icon";
import { FloatingList } from "./ModelPicker";
import { type ModelEffortPick, modelEffortChoices, modelEffortPick } from "./model_effort_logic";
import "./model_effort.css";

export interface ModelEffortProps {
  view: ModelEffortView;
  /** A line of the menu was chosen: a model, or a pair (model and effort). */
  onPick: (pick: ModelEffortPick) => void;
  /** An Effort level; `null` for Default (the tool's own). */
  onEffort: (effort: string | null) => void;
  /** Set up… / Set up <Tool>: Settings → Tools (`tool` is the one to set up, `null` from the menu). */
  onSetUp: (tool: string | null) => void;
}

export function ModelEffortField({ view, onPick, onEffort, onSetUp }: ModelEffortProps) {
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [width, setWidth] = useState<number>();
  const show = () => {
    setWidth(button.current?.offsetWidth);
    setOpen(true);
  };
  const value = view.value;
  const off = view.not_set_up;
  const effort = view.effort;
  const effortAt = effort?.levels.findIndex((l) => l.selected) ?? -1;
  return (
    <>
      <div className="fe-field me-field">
        <span className="fe-label">{view.model_label}</span>
        <button
          ref={button}
          type="button"
          className="select me-model"
          aria-label={view.model_label}
          aria-haspopup="listbox"
          aria-expanded={open}
          data-open={open || undefined}
          data-off={off !== null || undefined}
          onMouseDown={(e) => {
            if (open) e.preventDefault();
          }}
          onClick={() => {
            if (open) setOpen(false);
            else show();
          }}
          onKeyDown={(e) => {
            if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
            e.preventDefault();
            show();
          }}
        >
          {value.avatar !== null && <ToolAvatar avatar={value.avatar} />}
          <span className="select-value">
            <span>{value.text}</span>
          </span>
          {value.no_longer_listed !== null && (
            <span className="settings-model-detail settings-model-gone">{value.no_longer_listed}</span>
          )}
          {off !== null && <Icon name="triangle-alert" size={13} />}
          <Icon name="chevron-down" size={12} />
        </button>
        {off !== null ? (
          <div className="fe-fix me-off" data-tone="amber" role="note">
            <Icon name="triangle-alert" size={14} />
            <span className="fe-fix-text">
              <span>{off.text}</span>
              <span className="me-off-buttons">
                <Button
                  compact
                  variant="secondary"
                  label={off.set_up}
                  onClick={() => {
                    onSetUp(off.tool);
                  }}
                />
                <Button compact variant="ghost" label={off.pick} onClick={show} />
              </span>
            </span>
          </div>
        ) : (
          <span className="fe-hint">{view.hint}</span>
        )}
        {open && (
          <FloatingList
            anchor={button}
            minWidth={Math.max(width ?? 0, 300)}
            label={view.menu.title}
            choices={modelEffortChoices(view.menu)}
            className="menu settings-model-list me-menu"
            onChoose={(id) => {
              const pick = modelEffortPick(view.menu, id);
              if (pick !== undefined) onPick(pick);
            }}
            onClose={(how) => {
              setOpen(false);
              if (how === "key") button.current?.focus();
            }}
          >
            {view.menu.missing !== null && (
              <div className="me-missing" role="presentation">
                <span>{view.menu.missing}</span>
                <button
                  type="button"
                  className="settings-link"
                  // Keeps focus in the list until the click lands.
                  onMouseDown={(e) => {
                    e.preventDefault();
                  }}
                  onClick={() => {
                    setOpen(false);
                    onSetUp(null);
                  }}
                >
                  {view.menu.set_up}
                </button>
              </div>
            )}
            <div className="settings-choice-note me-foot" role="presentation">
              {view.menu.foot}
            </div>
          </FloatingList>
        )}
      </div>
      {effort !== null && (
        <div className="fe-field me-effort">
          <span className="fe-label">{effort.label}</span>
          <Segmented
            label={effort.label}
            options={effort.levels.map((l) => l.label)}
            selected={effortAt}
            onSelect={(i) => {
              const level = effort.levels[i];
              if (level === undefined || i === effortAt) return;
              onEffort(level.id === "" ? null : level.id);
            }}
          />
          <span className="fe-hint">{effort.hint}</span>
        </div>
      )}
    </>
  );
}
