// Model and Effort: the one way a model is picked (UX-65, UX-68, UX-75; canvas A-2…A-4). A stage's Who does it in Edit
// flow draws it; Done's model, Settings → Noyan's "Noyan starts on" and the Noyan panel's header draw the same field.
// The field shows the tool's avatar and "Codex · gpt-5.5"; its menu has "As in your other flows" on top (one click sets
// model and effort), then each tool set up on this Mac with `default` first, then the tools that aren't set up with
// Set up…. A model whose tool isn't set up is marked in amber with Set up <Tool> and Pick another model (A-4). Effort
// is the tool's levels, hidden for a tool with none. Every word is `ModelEffortView`'s (`otaq-view`'s
// `model_effort.rs`); this keeps only whether the menu is open. `ModelEffortButton` is the same menu behind a compact
// button (the Noyan panel's header, A-6): a short menu of pairs and "Another model or effort…", which opens the whole
// menu with the Effort levels under it.
import { useRef, useState } from "react";

import { Button, Segmented, ToolAvatar, Tooltip } from "../../components";
import type { ModelEffortMenu } from "../../generated/ModelEffortMenu";
import type { ModelEffortView } from "../../generated/ModelEffortView";
import type { ToolAvatarView } from "../../generated/ToolAvatarView";
import { Icon } from "../../theme/Icon";
import { FloatingList } from "./ModelPicker";
import {
  ANOTHER_ID,
  type ModelEffortPick,
  effortPick,
  modelEffortChoices,
  modelEffortPick,
} from "./model_effort_logic";
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
            <MenuFoot
              menu={view.menu}
              onSetUp={() => {
                setOpen(false);
                onSetUp(null);
              }}
            />
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

/** Under the menu's lines: the tools that aren't set up with Set up… (when `missing` is shown), and the foot. */
function MenuFoot({
  menu,
  missing = true,
  onSetUp,
}: {
  menu: ModelEffortMenu;
  missing?: boolean;
  onSetUp: () => void;
}) {
  return (
    <>
      {missing && menu.missing !== null && (
        <div className="me-missing" role="presentation">
          <span>{menu.missing}</span>
          <button
            type="button"
            className="settings-link"
            // Keeps focus in the list until the click lands.
            onMouseDown={(e) => {
              e.preventDefault();
            }}
            onClick={onSetUp}
          >
            {menu.set_up}
          </button>
        </div>
      )}
      <div className="settings-choice-note me-foot" role="presentation">
        {menu.foot}
      </div>
    </>
  );
}

export interface ModelEffortButtonProps extends ModelEffortProps {
  /** The closed button: the tool's avatar and "opus · max". */
  avatar: ToolAvatarView | null;
  text: string;
  /** Its tooltip and accessible name. */
  tooltip: string;
}

/** The menu behind a compact button (the Noyan panel's header, A-6): the short menu (the pairs, "Another model or
 * effort…") until "Another model or effort…" turns it into the whole one, with the Effort levels under the groups. */
export function ModelEffortButton({
  view,
  avatar,
  text,
  tooltip,
  onPick,
  onEffort,
  onSetUp,
}: ModelEffortButtonProps) {
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [full, setFull] = useState(false);
  // "Another model or effort…" was chosen: the list stays open, now whole, instead of closing on the choice.
  const staying = useRef(false);
  const show = () => {
    setFull(false);
    setOpen(true);
  };
  return (
    <>
      <Tooltip text={tooltip}>
        <button
          ref={button}
          type="button"
          className="me-compact"
          aria-label={tooltip}
          aria-haspopup="listbox"
          aria-expanded={open}
          data-open={open || undefined}
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
          {avatar !== null && <ToolAvatar avatar={avatar} />}
          <span className="me-compact-text">{text}</span>
          <Icon name="chevron-down" size={12} />
        </button>
      </Tooltip>
      {open && (
        <FloatingList
          anchor={button}
          minWidth={280}
          label={view.menu.title}
          choices={modelEffortChoices(view.menu, { full, effort: view.effort })}
          className="menu settings-model-list me-menu"
          onChoose={(id) => {
            if (id === ANOTHER_ID) {
              staying.current = true;
              setFull(true);
              return;
            }
            const level = effortPick(id);
            if (level !== undefined) {
              onEffort(level);
              return;
            }
            const pick = modelEffortPick(view.menu, id);
            if (pick !== undefined) onPick(pick);
          }}
          onClose={(how) => {
            if (staying.current) {
              staying.current = false;
              return;
            }
            setOpen(false);
            if (how === "key") button.current?.focus();
          }}
        >
          <MenuFoot
            menu={view.menu}
            missing={full}
            onSetUp={() => {
              setOpen(false);
              onSetUp(null);
            }}
          />
        </FloatingList>
      )}
    </>
  );
}

/** The model a started conversation runs on, read-only (UX-70: a new conversation is how to change it). */
export function ModelEffortLabel({
  avatar,
  text,
  tooltip,
}: {
  avatar: ToolAvatarView | null;
  text: string;
  tooltip: string;
}) {
  return (
    <Tooltip text={tooltip}>
      <span className="me-compact" data-read-only role="note" aria-label={tooltip}>
        {avatar !== null && <ToolAvatar avatar={avatar} />}
        <span className="me-compact-text">{text}</span>
      </span>
    </Tooltip>
  );
}
