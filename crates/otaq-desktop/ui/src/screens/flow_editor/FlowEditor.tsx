// The flow editor (T-0471; FED-01, FED-03, FED-04, FED-07; d0115, d0087; *27 → Row · One component* `s27-02`, *Row ·
// In onboarding* `s27-03`, *Row · The agent changes it* `s27-04`, *Row · In the project* `s27-05`, *Row · Changes that
// wait* `s27-07`): one component over the window in three parts. **Start from** (left, 230 px): the flow in use, the
// presets, Made for <project> with an Ask per connected tool, Import a flow…. **The flow** (middle): the animation of
// what is in the draft ("Watch it"; it folds while a step's details are open), the steps (drag a grip, or Alt+↑/↓, to
// reorder; + between them on hover; Add a stage), the selected step's details or the legend, the change line and Cancel
// · Apply. **The Project agent** (right, 360 px): "Not started" with Otaq's hint and fixed chips until the user types
// or clicks one, then its terminal. Everything it says is `flow_editor.view`'s; the body is its own component so
// onboarding's Flow step and the Flow setup page (`FlowEditorPage`, FED-09) and the gallery draw the same.
// A new stage's details start with its templates, and the Skill field offers Pick a skill… and Ask the agent to
// write it (T-0472; FED-05, FED-06; `s26-09`, `s27-05`, `s27-06`).
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";

import { keymap } from "../../app/keyboard";
import { bindingFor } from "../../app/keys";
import type { OverlayProps } from "../../app/registry/overlays";
import { useStore } from "../../app/store";
import {
  AnchoredMenu,
  Button,
  Checkbox,
  IconButton,
  Kbd,
  Popover,
  reducedMotion,
  Segmented,
  Select,
  StatusPill,
  TextInput,
  ToolAvatar,
  Tooltip,
  PLAIN_TEXT,
} from "../../components";
import type { FlowEditorAgent } from "../../generated/FlowEditorAgent";
import type { FlowEditorChipAction } from "../../generated/FlowEditorChipAction";
import type { FlowEditorDetails } from "../../generated/FlowEditorDetails";
import type { FlowEditorEntry } from "../../generated/FlowEditorEntry";
import type { FlowEditorFlow } from "../../generated/FlowEditorFlow";
import type { FlowEditorFrom } from "../../generated/FlowEditorFrom";
import type { FlowEditorMark } from "../../generated/FlowEditorMark";
import type { FlowEditorParams } from "../../generated/FlowEditorParams";
import type { FlowEditorPick } from "../../generated/FlowEditorPick";
import type { FlowEditorSkillField } from "../../generated/FlowEditorSkillField";
import type { FlowEditorTemplates } from "../../generated/FlowEditorTemplates";
import type { FlowEditorStageEdit } from "../../generated/FlowEditorStageEdit";
import type { FlowEditorStartFrom } from "../../generated/FlowEditorStartFrom";
import type { FlowEditorStep } from "../../generated/FlowEditorStep";
import type { FlowEditorTile } from "../../generated/FlowEditorTile";
import type { FlowEditorView } from "../../generated/FlowEditorView";
import { useServiceView } from "../../service";
import { pickFlowFile, request } from "../../tauri";
import { Terminal, type TerminalHandle } from "../../terminal/Terminal";
import { Icon } from "../../theme/Icon";
import {
  addOnOpening,
  applyDraft,
  askEach,
  askTool,
  cancelDraft,
  chipPressed,
  dropIndex,
  editorAgent,
  editorAgents,
  editStage,
  FLOW_EDITOR_EVENTS,
  importChoice,
  keyMove,
  nextTile,
  pickEntry,
  sendFirst,
  skillButton,
  TILE_MS,
  undo,
  writeSkill,
} from "./logic";

const ignore = () => undefined;

/** What the editor's clicks do: the overlay's calls, or the gallery's recorder. */
export interface EditorActions {
  select: (stage: string | null) => void;
  edit: (edit: FlowEditorStageEdit) => void;
  pick: (pick: FlowEditorPick) => void;
  undo: (stage: string | null) => void;
  importChoice: (id: string) => void;
  ask: (tool: string) => void;
  askBoth: (tools: readonly string[]) => void;
  chip: (action: FlowEditorChipAction, openChooser: () => void) => void;
  send: (text: string) => void;
  /** The Skill field's buttons on a stage: Ask the agent to write it, Stop, Undo, Open. */
  skill: (stage: string, button: SkillButton) => void;
  apply: () => void;
  cancel: () => void;
}

export type SkillButton = "write_skill" | "stop_writing" | "undo_skill" | "open_skill";

// ---------------------------------------------------------------------------------------------------------- Start from

function EntryRow({ entry, onPick }: { entry: FlowEditorEntry; onPick: () => void }) {
  return (
    <button type="button" className="fe-entry" data-chosen={entry.chosen || undefined} onClick={onPick}>
      <span className="fe-entry-top">
        {entry.avatar !== null && <ToolAvatar avatar={entry.avatar} />}
        <span className="fe-entry-name">{entry.name}</span>
        {entry.badge !== null && <StatusPill pill={entry.badge.pill} />}
        <span className="fe-spacer" />
        <span className="fe-entry-count">{entry.count}</span>
      </span>
      <span className="fe-entry-line">{entry.line}</span>
    </button>
  );
}

function StartFromColumn({ start, actions }: { start: FlowEditorStartFrom; actions: EditorActions }) {
  const importButton = useRef<HTMLButtonElement>(null);
  const [importOpen, setImportOpen] = useState(false);
  const made = start.made_for;
  return (
    <aside className="fe-start" aria-label={start.title}>
      <h3 className="fe-eyebrow">{start.title}</h3>
      {start.entries.map((entry) => (
        <EntryRow
          key={entry.key}
          entry={entry}
          onPick={() => {
            actions.pick(entry.pick);
          }}
        />
      ))}
      <div className="fe-rule" />
      <h3 className="fe-eyebrow fe-made-title">
        <Icon name="sparkles" size={13} />
        {made.title}
      </h3>
      <p className="fe-muted">{made.text}</p>
      {made.entries.map((entry) => (
        <EntryRow
          key={entry.key}
          entry={entry}
          onPick={() => {
            actions.pick(entry.pick);
          }}
        />
      ))}
      {made.tools.map((row) => (
        <div key={row.tool} className="fe-tool">
          <ToolAvatar avatar={row.avatar} />
          <span className="fe-tool-text">
            <span className="fe-tool-name">{row.name}</span>
            <span className="fe-muted">{row.status}</span>
          </span>
          {row.ask !== null && (
            <Button
              compact
              variant="secondary"
              label={row.ask}
              onClick={() => {
                actions.ask(row.tool);
              }}
            />
          )}
        </div>
      ))}
      <button
        ref={importButton}
        type="button"
        className="fe-import"
        aria-haspopup="menu"
        aria-expanded={importOpen}
        onClick={() => {
          setImportOpen((o) => !o);
        }}
      >
        <Icon name="folder-input" size={13} />
        {start.import.label}
      </button>
      {importOpen && (
        <AnchoredMenu
          anchor={importButton}
          align="start"
          label={start.import.label}
          items={start.import.menu}
          onChoose={(id) => {
            setImportOpen(false);
            actions.importChoice(id);
          }}
          onClose={(how) => {
            setImportOpen(false);
            if (how === "key") importButton.current?.focus();
          }}
        />
      )}
      <span className="fe-spacer-v" />
      <p className="fe-muted">{start.note}</p>
    </aside>
  );
}

// ----------------------------------------------------------------------------------------------------------- The flow

/** The animation: the walking ticket goes from tile to tile, and the caption under them follows it. */
export function FlowAnimation({ tiles }: { tiles: readonly FlowEditorTile[] }) {
  const [at, setAt] = useState(0);
  const still = useMemo(() => reducedMotion(), []);
  useEffect(() => {
    if (still) return;
    const timer = window.setInterval(() => {
      setAt((i) => nextTile(i, tiles.length));
    }, TILE_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [still, tiles.length]);
  const index = Math.min(at, Math.max(tiles.length - 1, 0));
  const tile = tiles[index];
  const width = tiles.length === 0 ? 100 : 100 / tiles.length;
  return (
    <div className="fe-anim" aria-hidden="true">
      <div className="fe-anim-tiles">
        {tiles.map((t, i) => (
          <div key={t.id} className="fe-anim-tile" data-on={i === index || undefined}>
            <span className="fe-anim-name">{t.name}</span>
            <span className="fe-anim-kind">
              <Icon name={t.icon} size={11} />
              {t.kind}
            </span>
          </div>
        ))}
      </div>
      {tile !== undefined && (
        <div className="fe-anim-lane">
          <div
            className="fe-anim-ticket"
            style={{ left: `calc(${String(width * index)}% + 6px)`, width: `calc(${String(width)}% - 12px)` }}
          >
            <span className="fe-anim-key">SHOP-1</span>
            <span className="fe-anim-state" data-palette={tile.tone.palette}>
              <span className="fe-anim-dot" />
              {tile.ticket}
            </span>
          </div>
        </div>
      )}
      {tile !== undefined && (
        <div className="fe-anim-caption">
          <span className="fe-anim-caption-title">
            <Icon name={tile.icon} size={13} />
            {tile.caption_title}
          </span>
          <span className="fe-anim-caption-text">
            {tile.caption_mark !== null && <strong>{tile.caption_mark} </strong>}
            {tile.caption}
          </span>
        </div>
      )}
    </div>
  );
}

function MarkTag({ mark, onUndo }: { mark: FlowEditorMark; onUndo: () => void }) {
  return (
    <span className="fe-mark">
      <Tooltip text={mark.why}>
        <span className="fe-mark-label">{mark.label}</span>
      </Tooltip>
      <button
        type="button"
        className="fe-mark-undo"
        onClick={(e) => {
          e.stopPropagation();
          onUndo();
        }}
      >
        <Icon name="undo-2" size={11} />
        {mark.undo}
      </button>
    </span>
  );
}

interface Drag {
  from: number;
  middles: number[];
  y: number;
}

function StepList({ flow, actions }: { flow: FlowEditorFlow; actions: EditorActions }) {
  const list = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const stages = flow.steps.filter((s) => s.id !== "ready");
  const startDrag = (index: number, e: React.PointerEvent) => {
    const rows = list.current?.querySelectorAll<HTMLElement>("[data-stage-index]") ?? [];
    const middles = Array.from(rows, (row) => {
      const r = row.getBoundingClientRect();
      return r.top + r.height / 2;
    });
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setDrag({ from: index, middles, y: e.clientY });
  };
  const target = drag === null ? null : dropIndex(drag.middles, drag.y, drag.from);
  return (
    <div className="fe-steps" ref={list}>
      {flow.steps.map((step) => {
        const index = stages.indexOf(step);
        return (
          <StepItem
            key={step.id}
            step={step}
            index={index}
            dragging={drag !== null && drag.from === index}
            dropBefore={drag !== null && target !== null && target === index && index !== drag.from}
            actions={actions}
            onGrip={(e) => {
              startDrag(index, e);
            }}
            onGripMove={(e) => {
              if (drag !== null) setDrag({ ...drag, y: e.clientY });
            }}
            onGripUp={() => {
              if (drag !== null && target !== null && target !== drag.from) {
                actions.edit({ edit: "move", stage: stages[drag.from]?.id ?? "", to: target });
              }
              setDrag(null);
            }}
            onKeyMove={(key) => {
              const to = keyMove(index, stages.filter((s) => s.draggable).length, key);
              if (to !== null) actions.edit({ edit: "move", stage: step.id, to });
            }}
          />
        );
      })}
      <button
        type="button"
        className="fe-add"
        onClick={() => {
          actions.edit(
            flow.add_stage_after === null ? { edit: "add" } : { edit: "add", after: flow.add_stage_after },
          );
        }}
      >
        <Icon name="plus" size={12} />
        {flow.add_stage}
      </button>
    </div>
  );
}

function StepItem({
  step,
  index,
  dragging,
  dropBefore,
  actions,
  onGrip,
  onGripMove,
  onGripUp,
  onKeyMove,
}: {
  step: FlowEditorStep;
  index: number;
  dragging: boolean;
  dropBefore: boolean;
  actions: EditorActions;
  onGrip: (e: React.PointerEvent) => void;
  onGripMove: (e: React.PointerEvent) => void;
  onGripUp: () => void;
  onKeyMove: (key: string) => void;
}) {
  const attrs = {
    className: "fe-step",
    "data-selected": step.selected || undefined,
    "data-marked": step.mark !== null || step.written !== null || undefined,
    "data-added": step.added || undefined,
    "data-problem": step.problem !== null || undefined,
    "data-dragging": dragging || undefined,
    "data-drop-before": dropBefore || undefined,
    ...(index >= 0 && { "data-stage-index": index }),
  };
  const body = (
    <>
      {step.draggable ? (
        <span
          className="fe-grip"
          aria-hidden="true"
          onPointerDown={(e) => {
            e.stopPropagation();
            onGrip(e);
          }}
          onPointerMove={onGripMove}
          onPointerUp={onGripUp}
          onPointerCancel={onGripUp}
        >
          <Icon name="grip-vertical" size={12} />
        </span>
      ) : (
        <span className="fe-grip fe-grip-off" aria-hidden="true">
          <Icon name="grip-vertical" size={12} />
        </span>
      )}
      <Icon name={step.icon} size={14} />
      <span className="fe-step-text">
        <span className="fe-step-name">{step.name}</span>
        <span className="fe-step-kind">{step.kind}</span>
        {step.problem !== null && <span className="fe-step-problem">{step.problem}</span>}
      </span>
      {step.mark !== null && (
        <MarkTag
          mark={step.mark}
          onUndo={() => {
            actions.undo(step.id);
          }}
        />
      )}
      {step.written !== null && (
        <MarkTag
          mark={step.written}
          onUndo={() => {
            actions.skill(step.id, "undo_skill");
          }}
        />
      )}
      {step.who !== null && (
        <span className="fe-step-who">
          {step.avatar !== null && <ToolAvatar avatar={step.avatar} />}
          {step.who}
        </span>
      )}
    </>
  );
  const row = step.selectable ? (
    <div
      {...attrs}
      role="button"
      tabIndex={0}
      aria-pressed={step.selected}
      onClick={(e) => {
        // A press on the grip drags; it doesn't select.
        if ((e.target as Element).closest(".fe-grip") !== null) return;
        actions.select(step.selected ? null : step.id);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          actions.select(step.selected ? null : step.id);
        } else if (e.altKey && step.draggable && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
          e.preventDefault();
          onKeyMove(e.key);
        }
      }}
    >
      {body}
    </div>
  ) : (
    <div {...attrs}>{body}</div>
  );
  return (
    <div className="fe-step-wrap">
      {step.caption !== null ? <Tooltip text={step.caption}>{row}</Tooltip> : row}
      {step.can_add_after && (
        <button
          type="button"
          className="fe-plus"
          aria-label={`Add a stage after ${step.name}`}
          onClick={() => {
            actions.edit(step.add_after === null ? { edit: "add" } : { edit: "add", after: step.add_after });
          }}
        >
          <Icon name="plus" size={11} />
        </button>
      )}
    </div>
  );
}

/**
 * The Skill field (FED-06): the stage's skill (click it to pick another); empty with Pick a skill… (the user's own,
 * searchable) and Ask the agent to write it; "Being written by the Project agent…" with Stop; or the skill the agent
 * wrote, tinted, with Open and Undo.
 */
export function SkillField({
  field,
  stage,
  actions,
}: {
  field: FlowEditorSkillField;
  stage: string;
  actions: EditorActions;
}) {
  const [picking, setPicking] = useState(false);
  const anchor = useRef<HTMLSpanElement>(null);
  const state = field.state;
  const pick = () => {
    setPicking(true);
  };
  let body: React.ReactNode;
  switch (state.state) {
    case "set":
      body = (
        <span ref={anchor} className="fe-skill-anchor">
          <button type="button" className="fe-value fe-skill fe-skill-set" onClick={pick}>
            <Icon name="scroll-text" size={13} />
            {state.skill}
            <Icon name="chevron-down" size={12} />
          </button>
        </span>
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
                actions.skill(stage, "write_skill");
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
          <button
            type="button"
            className="fe-link"
            onClick={() => {
              actions.skill(stage, "open_skill");
            }}
          >
            {state.open}
          </button>
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
      break;
  }
  return (
    <div className="fe-field">
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

/** A new stage's Start from (FED-05): the plain templates (name and kind) and the presets' (complete). */
function TemplatesBlock({
  templates,
  stage,
  actions,
}: {
  templates: FlowEditorTemplates;
  stage: string;
  actions: EditorActions;
}) {
  const row = (chips: FlowEditorTemplates["plain"]) => (
    <div className="fe-templates-row">
      {chips.map((chip) => (
        <button
          key={chip.template}
          type="button"
          className="fe-template"
          data-chosen={chip.chosen || undefined}
          aria-pressed={chip.chosen}
          onClick={() => {
            actions.edit({ edit: "template", stage, template: chip.template });
          }}
        >
          <Icon name={chip.icon} size={11} />
          {chip.label}
        </button>
      ))}
    </div>
  );
  return (
    <div className="fe-templates">
      <div className="fe-templates-group">
        <div className="fe-templates-head">
          <span className="fe-templates-label">{templates.label}</span>
          <span className="fe-muted">{templates.plain_note}</span>
        </div>
        {row(templates.plain)}
      </div>
      <div className="fe-templates-group">
        <div className="fe-templates-head">
          <span className="fe-templates-label">{templates.presets_label}</span>
          <span className="fe-muted">{templates.presets_note}</span>
        </div>
        {row(templates.presets)}
      </div>
    </div>
  );
}

/** The Name field: committed on Enter or when it loses focus (keyed by the stage and its name, so it starts afresh). */
function NameField({ details, onRename }: { details: FlowEditorDetails; onRename: (name: string) => void }) {
  const [name, setName] = useState(details.name);
  const commit = () => {
    if (name.trim() !== "" && name !== details.name) onRename(name);
    else setName(details.name);
  };
  return (
    <form
      className="fe-field"
      onSubmit={(e) => {
        e.preventDefault();
        commit();
      }}
    >
      <span className="fe-label">{details.name_label}</span>
      <TextInput value={name} label={details.name_label} onChange={setName} onBlur={commit} />
    </form>
  );
}

function DetailsPanel({ details, actions }: { details: FlowEditorDetails; actions: EditorActions }) {
  const stage = details.stage;
  const kindIndex = details.kinds.findIndex((k) => k.kind === details.kind);
  return (
    <section className="fe-details" aria-label={details.title}>
      <header className="fe-details-head">
        <Icon name={details.templates !== null ? "square-plus" : details.icon} size={15} />
        <h4>{details.templates?.title ?? details.title}</h4>
        {details.mark !== null && (
          <MarkTag
            mark={details.mark}
            onUndo={() => {
              actions.undo(stage);
            }}
          />
        )}
        <span className="fe-spacer" />
        {details.remove !== null && (
          <Button
            compact
            variant="ghost"
            icon="trash-2"
            label={details.remove}
            onClick={() => {
              actions.select(null);
              actions.edit({ edit: "remove", stage });
            }}
          />
        )}
        <IconButton
          icon="x"
          label="Close"
          onClick={() => {
            actions.select(null);
          }}
        />
      </header>
      {details.templates !== null && (
        <TemplatesBlock templates={details.templates} stage={stage} actions={actions} />
      )}
      <NameField
        key={`${details.stage}-${details.name}`}
        details={details}
        onRename={(name) => {
          actions.edit({ edit: "rename", stage, name });
        }}
      />
      <div className="fe-field">
        <span className="fe-label">{details.kind_label}</span>
        {details.fixed_kind !== null ? (
          <span className="fe-value">{details.fixed_kind}</span>
        ) : (
          <Segmented
            label={details.kind_label}
            options={details.kinds.map((k) => k.label)}
            selected={kindIndex}
            onSelect={(i) => {
              const kind = details.kinds[i]?.kind;
              if (kind !== undefined && i !== kindIndex) actions.edit({ edit: "kind", stage, kind });
            }}
          />
        )}
      </div>
      {details.agent !== null && (
        <div className="fe-field">
          <span className="fe-label">{details.agent.label}</span>
          <div className="fe-agent-row">
            {details.agent.avatar !== null && <ToolAvatar avatar={details.agent.avatar} />}
            <Select
              label={details.agent.label}
              value={details.agent.name}
              items={details.agent.items}
              onChange={(agent) => {
                if (agent !== details.agent?.name) actions.edit({ edit: "agent", stage, agent });
              }}
            />
            {details.agent.note !== null && <span className="fe-muted">{details.agent.note}</span>}
          </div>
          <span className="fe-hint">
            <Icon name="info" size={13} />
            {details.agent.hint}
          </span>
        </div>
      )}
      {details.skill !== null && (
        <SkillField key={stage} field={details.skill} stage={stage} actions={actions} />
      )}
      {details.can.length > 0 && (
        <div className="fe-field">
          <span className="fe-label">{details.can_label}</span>
          {details.can.map((row) => (
            <Checkbox
              key={row.option}
              checked={row.on}
              disabled={!row.enabled}
              label={row.label}
              onChange={(on) => {
                actions.edit({ edit: "option", stage, option: row.option, on });
              }}
            />
          ))}
        </div>
      )}
      {details.problem !== null && (
        <p className="fe-problem" role="alert">
          {details.problem}
        </p>
      )}
    </section>
  );
}

function Legend({ flow }: { flow: FlowEditorFlow }) {
  return (
    <section className="fe-legend">
      <p className="fe-legend-hint">{flow.hint}</p>
      <p className="fe-muted">{flow.legend_title}</p>
      {flow.legend.map((row) => (
        <div key={row.name} className="fe-legend-row">
          <Icon name={row.icon} size={14} />
          <span className="fe-legend-name">{row.name}</span>
          <span className="fe-muted">{row.text}</span>
        </div>
      ))}
    </section>
  );
}

function FlowColumn({
  view,
  titleId,
  busy,
  actions,
}: {
  view: FlowEditorView;
  titleId: string;
  busy: boolean;
  actions: EditorActions;
}) {
  const { flow, footer } = view;
  return (
    <main className="fe-flow">
      <header className="fe-flow-head">
        <div className="fe-flow-titles">
          <h2 id={titleId}>{flow.title}</h2>
          <p className="fe-flow-summary">{flow.summary}</p>
        </div>
        <span className="fe-spacer" />
        {flow.undo_all !== null && (
          <Button
            compact
            variant="ghost"
            icon="undo-2"
            label={flow.undo_all}
            onClick={() => {
              actions.undo(null);
            }}
          />
        )}
        <button
          type="button"
          className="fe-watch"
          onClick={() => {
            if (flow.folded) actions.select(null);
          }}
        >
          <Icon name="play" size={12} />
          {flow.watch}
        </button>
      </header>
      {flow.folded ? (
        <div className="fe-folded">
          <Icon name="fold-horizontal" size={12} />
          {flow.fold_note}
        </div>
      ) : (
        <FlowAnimation tiles={flow.tiles} />
      )}
      <div className="fe-flow-body">
        <StepList flow={flow} actions={actions} />
        {flow.details !== null ? (
          <DetailsPanel details={flow.details} actions={actions} />
        ) : (
          <Legend flow={flow} />
        )}
      </div>
      {flow.problems.map((p) => (
        <p key={p} className="fe-problem" role="alert">
          {p}
        </p>
      ))}
      {flow.change_line !== null && (
        <p className="fe-change">
          <Icon name="info" size={13} />
          {flow.change_line}
        </p>
      )}
      <footer className="fe-footer">
        <Button variant="secondary" label={footer.cancel} disabled={busy} onClick={actions.cancel} />
        <span className="fe-spacer" />
        {footer.reason !== null && (
          <span className="fe-reason">
            <Icon name="triangle-alert" size={13} />
            {footer.reason}
          </span>
        )}
        <Button
          label={footer.apply}
          disabled={busy || !footer.apply_enabled}
          loading={busy}
          onClick={actions.apply}
        />
      </footer>
    </main>
  );
}

// ------------------------------------------------------------------------------------------------- The Project agent

function Chooser({
  agent,
  anchor,
  actions,
  onClose,
}: {
  agent: FlowEditorAgent;
  anchor: React.RefObject<HTMLElement | null>;
  actions: EditorActions;
  onClose: () => void;
}) {
  const chooser = agent.chooser;
  if (chooser === null) return null;
  return (
    <Popover label={chooser.title} anchor={anchor} align="start" onClose={onClose}>
      <div className="fe-chooser">
        <h4>{chooser.title}</h4>
        <p className="fe-muted">{chooser.text}</p>
        {chooser.tools.map((t) => (
          <button
            key={t.tool}
            type="button"
            className="fe-chooser-row"
            onClick={() => {
              onClose();
              actions.ask(t.tool);
            }}
          >
            <ToolAvatar avatar={t.avatar} />
            <span className="fe-tool-text">
              <span className="fe-tool-name">{t.name}</span>
              <span className="fe-muted">{t.detail}</span>
            </span>
          </button>
        ))}
        {chooser.both !== null && (
          <button
            type="button"
            className="fe-chooser-row"
            onClick={() => {
              onClose();
              actions.askBoth(chooser.tools.map((t) => t.tool));
            }}
          >
            <Icon name="git-compare" size={14} />
            <span className="fe-tool-text">
              <span className="fe-tool-name">{chooser.both[0]}</span>
              <span className="fe-muted">{chooser.both[1]}</span>
            </span>
          </button>
        )}
        <p className="fe-muted fe-chooser-foot">{chooser.foot}</p>
      </div>
    </Popover>
  );
}

export function AgentColumn({
  agent,
  actions,
  input,
  terminal,
}: {
  agent: FlowEditorAgent;
  actions: EditorActions;
  input: React.RefObject<HTMLInputElement | null>;
  terminal: React.Ref<TerminalHandle>;
}) {
  const [text, setText] = useState("");
  const [choosing, setChoosing] = useState(false);
  // The first message was sent from here: the agent's terminal takes the keyboard once it opens (#51, d0126).
  const [sent, setSent] = useState(false);
  const chipsRow = useRef<HTMLDivElement>(null);
  return (
    <aside className="fe-agent" aria-label={agent.title}>
      <header className="fe-agent-head">
        <Icon name="bot" size={15} />
        <h3>{agent.title}</h3>
        <StatusPill pill={agent.pill} />
      </header>
      <div className="fe-agent-line">
        {agent.avatar !== null && <ToolAvatar avatar={agent.avatar} />}
        <span>{agent.line}</span>
        <span className="fe-muted">{agent.note}</span>
      </div>
      <div className="fe-agent-body">
        {agent.session !== null ? (
          <div className="fe-agent-terminal">
            <Terminal
              key={agent.session}
              session={agent.session}
              label={agent.title}
              focusWhenLive={sent}
              ref={terminal}
            />
          </div>
        ) : (
          // A click anywhere in the idle panel is a click in its prompt (#51: a click beside the `>` line left the
          // keyboard where it was).
          // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- the prompt inside takes the keys; this only widens where a click lands
          <div
            className="fe-agent-idle"
            onClick={(e) => {
              if (e.target !== input.current) input.current?.focus();
            }}
          >
            {agent.hint.map((line) => (
              <p key={line} className="fe-agent-hint">
                {line}
              </p>
            ))}
            <label className="fe-agent-prompt">
              <span aria-hidden="true">&gt;</span>
              <input
                {...PLAIN_TEXT}
                ref={input}
                value={text}
                aria-label={agent.title}
                onChange={(e) => {
                  setText(e.target.value);
                }}
                onKeyDown={(e) => {
                  if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
                  e.preventDefault();
                  if (text.trim() === "") return;
                  actions.send(text);
                  setSent(true);
                  setText("");
                }}
              />
            </label>
          </div>
        )}
      </div>
      {agent.chips.length > 0 && (
        <div className="fe-chips" ref={chipsRow}>
          {agent.chips.map((chip) => (
            <button
              key={chip.label}
              type="button"
              className="fe-chip"
              onClick={() => {
                actions.chip(chip.action, () => {
                  setChoosing(true);
                });
              }}
            >
              <Icon name="message-circle" size={11} />
              {chip.label}
            </button>
          ))}
        </div>
      )}
      {choosing && (
        <Chooser
          agent={agent}
          anchor={chipsRow}
          actions={actions}
          onClose={() => {
            setChoosing(false);
          }}
        />
      )}
      <footer className="fe-agent-foot">
        <Icon name="keyboard" size={12} />
        <span className="fe-accent">{agent.foot}</span>
        <span className="fe-spacer" />
        {agent.keys !== null && <Kbd keys={agent.keys} />}
        <span className="fe-muted">{agent.keys_label}</span>
      </footer>
    </aside>
  );
}

// ------------------------------------------------------------------------------------------------------------ Whole

export interface FlowEditorBodyProps {
  view: FlowEditorView;
  busy?: boolean;
  actions: EditorActions;
  /** The idle panel's box and the running agent's terminal, for ⌘L. */
  input?: React.RefObject<HTMLInputElement | null>;
  terminal?: React.Ref<TerminalHandle>;
  titleId?: string;
}

/** The editor's three parts for a loaded view (also what the gallery and onboarding's Flow step draw). */
export function FlowEditorBody({
  view,
  busy = false,
  actions,
  input,
  terminal,
  titleId,
}: FlowEditorBodyProps) {
  const fallbackInput = useRef<HTMLInputElement>(null);
  const ownId = useId();
  return (
    <div className="fe">
      <StartFromColumn start={view.start_from} actions={actions} />
      <FlowColumn view={view} titleId={titleId ?? ownId} busy={busy} actions={actions} />
      <AgentColumn
        agent={view.agent}
        actions={actions}
        input={input ?? fallbackInput}
        terminal={terminal ?? null}
      />
    </div>
  );
}

/** Where the editor opens and what ends it: the overlay closes; onboarding's Flow step and the Flow setup page move on. */
interface EditorOpen {
  project: string;
  from: FlowEditorFrom;
  select_stage?: string;
  /** A stage to add a new one after on opening; `null`: first, after Ready for Dev. */
  add_after?: string | null;
  /** Apply (Use this flow) or Cancel (Skip, use the default) was answered: the agent the panel showed, if any. */
  onDone: (agent: string | null) => void;
  /** Esc with no step selected: the overlay closes; embedded, nothing. */
  onEscape?: () => void;
}

/**
 * The editor's state for one project: its view (`flow_editor.view` with the page's selection and the panel's agent),
 * the clicks, ⌘L into the panel and Esc (the details first). The overlay and the embedded page draw it the same way.
 */
function useFlowEditor(open: EditorOpen, box: React.RefObject<HTMLElement | null>) {
  const { project, from, onDone, onEscape } = open;
  const [selected, setSelected] = useState<string | null>(open.select_stage ?? null);
  const [busy, setBusy] = useState(false);
  const agents = useStore(editorAgents);
  const agent = agents[project] ?? null;
  const input = useRef<HTMLInputElement>(null);
  const terminal = useRef<TerminalHandle>(null);
  // A stage to add on opening (a column's Add a stage after <stage>): once, whatever React re-runs.
  const added = useRef(false);
  const addAfter = open.add_after;
  useEffect(() => {
    if (addAfter === undefined || added.current) return;
    added.current = true;
    editStage(project, addOnOpening(addAfter)).then((id) => {
      if (id !== null) setSelected(id);
    }, ignore);
  }, [project, addAfter]);
  const load = useCallback(() => {
    const p: FlowEditorParams = {
      project,
      from,
      ...(selected !== null && { selected }),
      ...(agent !== null && { agent }),
    };
    return request("flow_editor.view", p) as Promise<FlowEditorView>;
  }, [project, from, selected, agent]);
  const state = useServiceView(load, FLOW_EDITOR_EVENTS);
  const view = state.view;

  const finish = useCallback(
    (sent: Promise<unknown>) => {
      setBusy(true);
      sent.then(
        () => {
          setBusy(false);
          onDone(editorAgent(project));
        },
        () => {
          setBusy(false);
        },
      );
    },
    [onDone, project],
  );
  const actions = useMemo<EditorActions | null>(() => {
    if (view === undefined) return null;
    return {
      select: setSelected,
      edit: (edit) => {
        editStage(project, edit).then((id) => {
          if (id !== null) setSelected(id);
        }, ignore);
      },
      pick: (pick) => {
        setSelected(null);
        pickEntry(project, pick).catch(ignore);
      },
      undo: (stage) => {
        undo(project, stage).catch(ignore);
      },
      importChoice: (id) => {
        setSelected(null);
        importChoice(project, id, () => pickFlowFile(view.start_from.import.file_title)).catch(ignore);
      },
      ask: (tool) => {
        askTool(project, from, tool).catch(ignore);
      },
      askBoth: (tools) => {
        askEach(project, from, tools).catch(ignore);
      },
      chip: (action, openChooser) => {
        chipPressed(project, from, action, openChooser).catch(ignore);
      },
      send: (text) => {
        sendFirst(project, text).catch(ignore);
      },
      skill: (stage, button) => {
        const sent =
          button === "write_skill" ? writeSkill(project, stage) : skillButton(project, stage, button);
        sent.catch(ignore);
      },
      apply: () => {
        finish(applyDraft(view));
      },
      cancel: () => {
        finish(cancelDraft(view));
      },
    };
  }, [view, project, from, finish]);

  // ⌘L (the user's Focus the Project agent key) moves into the panel; Esc closes the details, then the editor.
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      const map = keymap.get();
      if (map !== undefined && bindingFor(map, e, false)?.action.action === "focus_pane") {
        e.preventDefault();
        if (input.current !== null) input.current.focus();
        else terminal.current?.focus();
        return;
      }
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (selected === null && onEscape === undefined) return;
      e.preventDefault();
      e.stopPropagation();
      if (selected !== null) setSelected(null);
      else onEscape?.();
    };
    el.addEventListener("keydown", onKey);
    return () => {
      el.removeEventListener("keydown", onKey);
    };
  }, [box, selected, onEscape]);

  return { state, view, actions, busy, input, terminal };
}

/**
 * The `flow_editor` overlay over the whole window: the project's draft with the page's selection and the panel's
 * agent. Esc closes the details first, then the editor (the draft stays for next time; Cancel drops it).
 */
export function FlowEditorOverlay({ params, close }: OverlayProps<"flow_editor">) {
  return <Editor key={`${params.project}-${params.from}`} params={params} close={close} />;
}

function Editor({ params, close }: OverlayProps<"flow_editor">) {
  const titleId = useId();
  const dialog = useRef<HTMLElement>(null);
  const editor = useFlowEditor({ ...params, onDone: close, onEscape: close }, dialog);
  const { state, view, actions } = editor;
  useEffect(() => {
    dialog.current?.focus();
  }, []);

  return (
    <div
      className="fe-scrim"
      role="presentation"
      // A press on the dim around it closes the editor, as Esc does (the draft stays in the service).
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <section
        ref={dialog}
        className="fe-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        {state.error !== undefined && view === undefined && (
          <p className="fe-problem" role="alert">
            {state.error.message}
          </p>
        )}
        {view !== undefined && actions !== null && (
          <FlowEditorBody
            view={view}
            busy={editor.busy}
            actions={actions}
            input={editor.input}
            terminal={editor.terminal}
            titleId={titleId}
          />
        )}
      </section>
    </div>
  );
}

export interface FlowEditorPageProps {
  project: string;
  /** Use this flow or Skip, use the default was answered: the agent the panel showed, if any. */
  onDone: (agent: string | null) => void;
}

/**
 * The editor as onboarding's Flow step and a later project's Flow setup page (FED-09, `s27-03`): in place, from Otaq
 * default, with Use this flow · Skip, use the default. The project stays in Flow setup until one is pressed.
 */
export function FlowEditorPage({ project, onDone }: FlowEditorPageProps) {
  const titleId = useId();
  const page = useRef<HTMLElement>(null);
  const editor = useFlowEditor({ project, from: "onboarding", onDone }, page);
  const { state, view, actions } = editor;
  return (
    <section ref={page} className="fe-page" aria-labelledby={titleId}>
      {state.error !== undefined && view === undefined && (
        <p className="fe-problem" role="alert">
          {state.error.message}
        </p>
      )}
      {view !== undefined && actions !== null && (
        <FlowEditorBody
          view={view}
          busy={editor.busy}
          actions={actions}
          input={editor.input}
          terminal={editor.terminal}
          titleId={titleId}
        />
      )}
    </section>
  );
}

/** For the gallery: the body with every click recorded. */
export function recordingActions(record: (what: string) => void): EditorActions {
  return {
    select: (stage) => {
      record(`select ${stage ?? "none"}`);
    },
    edit: (edit) => {
      record(`edit ${edit.edit}`);
    },
    pick: (pick) => {
      record(`pick ${pick.pick}`);
    },
    undo: (stage) => {
      record(`undo ${stage ?? "all"}`);
    },
    importChoice: (id) => {
      record(`import ${id}`);
    },
    ask: (tool) => {
      record(`ask ${tool}`);
    },
    askBoth: (tools) => {
      record(`ask ${tools.join(", ")}`);
    },
    chip: (action, openChooser) => {
      if (action.kind === "choose_tool") openChooser();
      record(`chip ${action.kind}`);
    },
    send: (text) => {
      record(`send ${text}`);
    },
    skill: (stage, button) => {
      record(`${button} ${stage}`);
    },
    apply: () => {
      record("apply");
    },
    cancel: () => {
      record("cancel");
    },
  };
}
