// The pickers of Settings → Agents (T-0410, from T-0343 / T-0366; AGT-02, AGM-03, d0086): one agent's Model field (*15 →
// Add agent · model picker open* `s15-30`), a list under the field with only what the tool's own list has, nothing
// typed, or while that list isn't there its loading or couldn't-load line with Retry (`GTuGI`), and "no longer listed"
// on the field (`OCna6`); and the Agents page's dropdown of a tool and a model (the default model: every connected
// tool under its name, then its models). Every row comes from the Rust views
// (`model_picker.rs`, `agents.rs`); this keeps only whether the list is open and which row is highlighted. The keys are
// the menus' (`menuKey`): ↑/↓ move, Home/End, ↵ or Space picks, Esc or Tab closes.
import {
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";

import { FloatingPortal, useFloatingStyle } from "../../components";
import { menuKey } from "../../components/logic";
import type { AgentsModelChoice } from "../../generated/AgentsModelChoice";
import type { AgentsModelDropdown } from "../../generated/AgentsModelDropdown";
import type { IconName } from "../../generated/IconName";
import type { MenuItem } from "../../generated/MenuItem";
import type { ModelPickerStatus } from "../../generated/ModelPickerStatus";
import type { ModelPickerView } from "../../generated/ModelPickerView";
import { Icon } from "../../theme/Icon";

/** One line of a list: a choice, a group's heading, or a muted note in place of a group's choices. */
export interface Choice {
  kind: "option" | "heading" | "note";
  id: string;
  label: string;
  /** The muted part: "now claude-opus-5-5", "Opus · always the newest". */
  detail: string | null;
  /** Drawn in the mono font (an exact model id). */
  mono: boolean;
  detailMono: boolean;
  checked: boolean;
  icon: IconName | null;
}

/** The id the model picker uses for the tool default (its value is none: Otaq passes no model). */
export const TOOL_DEFAULT_ID = "";

/** The model picker's lines, in the view's order: Tool default, ALWAYS THE NEWEST, EXACT MODELS. */
export function modelChoices(picker: ModelPickerView): Choice[] {
  return picker.groups.flatMap((group): Choice[] => [
    ...(group.heading === null
      ? []
      : [
          {
            kind: "heading" as const,
            id: `h-${group.heading}`,
            label: group.heading,
            detail: null,
            mono: false,
            detailMono: false,
            checked: false,
            icon: null,
          },
        ]),
    ...group.items.map((item) => ({
      kind: "option" as const,
      id: item.value ?? TOOL_DEFAULT_ID,
      label: item.label,
      detail: item.detail,
      mono: item.mono,
      // "now claude-opus-5-5" names an exact model.
      detailMono: item.value !== null,
      checked: item.selected,
      icon: null,
    })),
    ...(group.empty === null
      ? []
      : [
          {
            kind: "note" as const,
            id: `n-${group.empty}`,
            label: group.empty,
            detail: null,
            mono: false,
            detailMono: false,
            checked: false,
            icon: null,
          },
        ]),
  ]);
}

/** The lines as menu rows, for the menus' key rules (headings and notes are never chosen). */
function asMenu(choices: readonly Choice[]): MenuItem[] {
  return choices.map((c) => ({
    id: c.id,
    label: c.label,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: c.checked,
    header: c.kind !== "option",
  }));
}

export type ListClose = "key" | "outside";

/** The open list: takes focus, starts on the picked line, and gives back the picked id. */
export function ChoiceList({
  label,
  choices,
  className,
  onChoose,
  onClose,
  children,
}: {
  label: string;
  choices: readonly Choice[];
  className: string;
  onChoose: (id: string) => void;
  onClose: (how: ListClose) => void;
  /** Drawn after the choices: the list's loading or couldn't-load line. */
  children?: ReactNode;
}) {
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const items = asMenu(choices);
  const [highlighted, setHighlighted] = useState<number | null>(() => {
    const at = choices.findIndex((c) => c.checked);
    return at < 0 ? null : at;
  });
  useEffect(() => {
    root.current?.focus();
  }, []);
  const onKeyDown = (e: KeyboardEvent) => {
    const result = menuKey(items, highlighted, e.key);
    switch (result.kind) {
      case "highlight":
        setHighlighted(result.index);
        break;
      case "choose":
        onChoose(result.id);
        onClose("key");
        break;
      case "close":
        onClose("key");
        break;
      case "none":
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  };
  const row = (n: number) => `${id}-${String(n)}`;
  return (
    <div
      ref={root}
      className={className}
      role="listbox"
      aria-label={label}
      tabIndex={-1}
      aria-activedescendant={highlighted === null ? undefined : row(highlighted)}
      onKeyDown={onKeyDown}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) onClose("outside");
      }}
    >
      {choices.map((c, i) => {
        if (c.kind === "heading")
          return (
            <div key={c.id} className="menu-header" role="presentation">
              {c.label}
            </div>
          );
        if (c.kind === "note")
          return (
            <div key={c.id} className="settings-choice-note" role="presentation">
              {c.label}
            </div>
          );
        return (
          <button
            key={c.id}
            id={row(i)}
            type="button"
            tabIndex={-1}
            role="option"
            aria-selected={c.checked}
            className="menu-item settings-choice"
            data-highlighted={i === highlighted || undefined}
            // Keeps focus on the list, so the press isn't lost to its blur.
            onMouseDown={(e) => {
              e.preventDefault();
            }}
            onMouseMove={() => {
              if (i !== highlighted) setHighlighted(i);
            }}
            onClick={() => {
              onChoose(c.id);
              onClose("key");
            }}
          >
            <span className="menu-item-line">
              {c.icon !== null && <Icon name={c.icon} size={13} />}
              <span className="settings-choice-label" data-mono={c.mono || undefined}>
                {c.label}
              </span>
              {c.detail !== null && (
                <span className="settings-choice-detail" data-mono={c.detailMono || undefined}>
                  {c.detail}
                </span>
              )}
              {c.checked && <Icon name="check" size={13} label="current" />}
            </span>
          </button>
        );
      })}
      {children}
    </div>
  );
}

/** The keys that open a closed list from its field. */
function opensList(key: string): boolean {
  return key === "ArrowDown" || key === "ArrowUp";
}

/** The open list's line while the tool's list isn't there (`GTuGI`): a spinner and "Loading Codex's models…" over
 * three grey bars, or "Couldn't load Copilot's models · Retry". */
function ListStatus({ status, onRetry }: { status: ModelPickerStatus; onRetry: (tool: string) => void }) {
  const tool = status.retry_tool;
  return (
    <div className="settings-model-status" role="status" data-loading={status.loading || undefined}>
      <span className="settings-model-status-line">
        <Icon name={status.loading ? "loader" : "server-off"} size={13} />
        <span>{status.text}</span>
        {status.retry_label !== null && tool !== null && (
          <>
            <span className="settings-agents-dot">·</span>
            <button
              type="button"
              className="settings-link"
              // Keeps focus in the list, so it stays open while the list is read again.
              onMouseDown={(e) => {
                e.preventDefault();
              }}
              onClick={() => {
                onRetry(tool);
              }}
            >
              {status.retry_label}
            </button>
          </>
        )}
      </span>
      {status.loading && (
        <span className="settings-model-skeleton" aria-hidden>
          <span />
          <span />
          <span />
        </span>
      )}
    </div>
  );
}

/** The dialog's Model field: a button with the picked model and what it runs today ("no longer listed" in amber when
 * the tool's list dropped it), and the list floating under it, at least as wide as it. Nothing in it takes text.
 * `onRetry` reads the tool's list again (`settings.agent_models_retry`). */
export function ModelPicker({
  id,
  label,
  picker,
  onPick,
  onRetry,
}: {
  id?: string;
  label: string;
  picker: ModelPickerView;
  onPick: (model: string | null) => void;
  onRetry: (tool: string) => void;
}) {
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [width, setWidth] = useState<number>();
  const value = picker.value;
  return (
    <div className="settings-model-picker">
      <button
        ref={button}
        id={id}
        type="button"
        className="select settings-model-field"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        data-open={open || undefined}
        onMouseDown={(e) => {
          if (open) e.preventDefault();
        }}
        onClick={() => {
          setWidth(button.current?.offsetWidth);
          setOpen((o) => !o);
        }}
        onKeyDown={(e) => {
          if (!opensList(e.key)) return;
          e.preventDefault();
          setWidth(button.current?.offsetWidth);
          setOpen(true);
        }}
      >
        <span className="select-value" data-mono={value.mono || undefined}>
          <span>{value.label}</span>
        </span>
        {value.no_longer_listed !== null ? (
          <span className="settings-model-detail settings-model-gone">{value.no_longer_listed}</span>
        ) : (
          value.detail !== null && <span className="settings-model-detail">{value.detail}</span>
        )}
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <FloatingList
          anchor={button}
          minWidth={width}
          label={label}
          choices={modelChoices(picker)}
          className="menu settings-model-list"
          onChoose={(picked) => {
            onPick(picked === TOOL_DEFAULT_ID ? null : picked);
          }}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") button.current?.focus();
          }}
        >
          {picker.status && <ListStatus status={picker.status} onRetry={onRetry} />}
        </FloatingList>
      )}
    </div>
  );
}

/** The open dropdown, fixed under its field (so no dialog or scrolling page clips or pushes it) and as wide as it,
 * or at least `minWidth` wide. */
function FloatingList({
  anchor,
  width,
  minWidth,
  className = "menu",
  children,
  ...list
}: {
  anchor: RefObject<HTMLElement | null>;
  width?: number;
  minWidth?: number;
  className?: string;
  label: string;
  choices: readonly Choice[];
  onChoose: (id: string) => void;
  onClose: (how: ListClose) => void;
  children?: ReactNode;
}) {
  const floating = useRef<HTMLDivElement>(null);
  const style = useFloatingStyle(anchor, floating, "start", () => {
    list.onClose("outside");
  });
  return (
    <FloatingPortal>
      <div ref={floating} className="floating" style={{ ...style, width, minWidth }}>
        <ChoiceList {...list} className={className}>
          {children}
        </ChoiceList>
      </div>
    </FloatingPortal>
  );
}

/** A closed field showing `value` that opens `choices` in a dropdown as wide as it; read-only (no chevron) when there
 * is nothing to choose. */
export function Dropdown({
  id,
  label,
  value,
  choices,
  readOnly,
  className,
  onChoose,
}: {
  id?: string;
  label: string;
  value: ReactNode;
  choices: () => Choice[];
  readOnly: boolean;
  className: string;
  onChoose: (id: string) => void;
}) {
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [width, setWidth] = useState<number>();
  if (readOnly)
    return (
      <span id={id} className={`select ${className}`} data-read-only aria-label={label} role="note">
        {value}
      </span>
    );
  return (
    <>
      <button
        ref={button}
        id={id}
        type="button"
        className={`select ${className}`}
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        data-open={open || undefined}
        onMouseDown={(e) => {
          if (open) e.preventDefault();
        }}
        onClick={() => {
          setWidth(button.current?.offsetWidth);
          setOpen((o) => !o);
        }}
        onKeyDown={(e) => {
          if (!opensList(e.key)) return;
          e.preventDefault();
          setWidth(button.current?.offsetWidth);
          setOpen(true);
        }}
      >
        {value}
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <FloatingList
          anchor={button}
          width={width}
          label={label}
          choices={choices()}
          onChoose={onChoose}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") button.current?.focus();
          }}
        />
      )}
    </>
  );
}

/** A tool and a model, as the page sends it back; `null`: a line without a tool. */
export type ModelPick = { tool: string; model: string | null } | null;

/** The dropdown's lines: each tool's name as a heading, its models, and a note while its list isn't there. */
export function dropdownChoices(choices: readonly AgentsModelChoice[]): Choice[] {
  return choices.map((c) => ({
    kind: c.kind === "heading" ? "heading" : c.kind === "note" ? "note" : "option",
    id: c.id,
    label: c.label,
    detail: c.detail,
    mono: c.mono,
    detailMono: c.tool !== null && c.model !== null,
    checked: c.selected,
    icon: null,
  }));
}

/** What a chosen line of the dropdown picks. */
export function pickOf(choices: readonly AgentsModelChoice[], id: string): ModelPick | undefined {
  const c = choices.find((choice) => choice.id === id);
  if (c?.kind !== "option") return undefined;
  return c.tool === null ? null : { tool: c.tool, model: c.model };
}

/** The default model (AGM-03): "Claude Code · sonnet ▾" over every connected tool's list. */
export function ModelDropdown({
  label,
  dropdown,
  onPick,
}: {
  label: string;
  dropdown: AgentsModelDropdown;
  onPick: (pick: ModelPick) => void;
}) {
  return (
    <Dropdown
      label={label}
      value={
        <span className="select-value">
          <span>{dropdown.value}</span>
        </span>
      }
      choices={() => dropdownChoices(dropdown.choices)}
      readOnly={dropdown.read_only}
      className="settings-agents-dropdown"
      onChoose={(id) => {
        const pick = pickOf(dropdown.choices, id);
        if (pick !== undefined) onPick(pick);
      }}
    />
  );
}
