// A ticket's stage choices and its path (AGM-09; d0108, d0077; *08 → Row · Stage choices (update AG §9)*, *05 → Row
// · Stage choices and path*), as `stage_choices.rs` words them. Choices: a select per stage with options ("Dev: Rust
// developer ▾"), a lock once the stage started, and the line of a change someone made (an agent's violet with its
// reason, yours neutral). The form look keeps the picks made before the ticket exists (`picks`); the sheet's are sent
// at once. The path: the one-line summary, then a row per stage with its state's icon, its agent and model, a lock on
// a stage that ran. Never turquoise (d0077).
import type { MenuItem } from "../generated/MenuItem";
import type { StageChoiceRow } from "../generated/StageChoiceRow";
import type { StageChoicesView } from "../generated/StageChoicesView";
import type { TicketPathRow } from "../generated/TicketPathRow";
import type { TicketPathView } from "../generated/TicketPathView";
import { Icon } from "../theme/Icon";
import { Select } from "./Select";
import { Tooltip } from "./Tooltip";
import "./StageChoices.css";

export type StageChoicesLook = "form" | "sheet";

/** A row as shown with the form's pick on it (the picked word, checked in the menu); the row as the view gave it
 * without one. */
export function shownChoice(row: StageChoiceRow, pick: string | undefined): StageChoiceRow {
  const item = pick === undefined ? undefined : row.items.find((i) => i.id === pick);
  if (!item) return row;
  return {
    ...row,
    value: item.label,
    items: row.items.map((i): MenuItem => ({ ...i, checked: i.id === pick })),
  };
}

function Lock({ text }: { text: string }) {
  return (
    <Tooltip text={text}>
      {/* Focusable, so the keyboard reaches the tooltip too. */}
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- the lock explains itself on focus */}
      <span className="stage-choice-lock" tabIndex={0} role="img" aria-label={text}>
        <Icon name="lock" size={12} />
      </span>
    </Tooltip>
  );
}

function ChoiceLine({
  row,
  onPick,
}: {
  row: StageChoiceRow;
  onPick: (stage: string, choice: string) => void;
}) {
  return (
    <div className="stage-choice">
      <div className="stage-choice-line">
        <span className="stage-choice-label">{row.label}</span>
        {row.lock !== null || row.items.length === 0 ? (
          <span className="stage-choice-value" data-locked>
            <span>{row.value}</span>
            {row.lock !== null && <Lock text={row.lock} />}
          </span>
        ) : (
          <span className="stage-choice-select">
            <Select
              label={`${row.label} ${row.value}`}
              value={row.value}
              items={row.items}
              onChange={(choice) => {
                if (choice !== row.value) onPick(row.stage_id, choice);
              }}
            />
          </span>
        )}
      </div>
      {row.change !== null && (
        <div className="stage-choice-change" data-agent={row.change.agent_made || undefined}>
          <Icon name={row.change.icon} size={12} />
          <span>{row.change.text}</span>
        </div>
      )}
    </div>
  );
}

export interface StageChoicesProps {
  view: StageChoicesView;
  look: StageChoicesLook;
  /** The form's picks not saved yet, by stage id. */
  picks?: Readonly<Record<string, string>>;
  /** A pick: the stage's id and the word it sends. */
  onPick: (stage: string, choice: string) => void;
}

/** The ⌘N form's "Stage choices" field, or the sheet's Choices. */
export function StageChoices({ view, look, picks, onPick }: StageChoicesProps) {
  return (
    <div className="stage-choices" data-look={look}>
      <div className="stage-choices-head">
        <span className="stage-choices-heading">{view.heading}</span>
        {view.set_by !== null && <span className="stage-choices-by">{view.set_by}</span>}
      </div>
      {view.rows.map((row) => (
        <ChoiceLine key={row.stage_id} row={shownChoice(row, picks?.[row.stage_id])} onPick={onPick} />
      ))}
      {view.note !== null && <p className="stage-choices-note">{view.note}</p>}
    </div>
  );
}

function PathLine({ row }: { row: TicketPathRow }) {
  return (
    <div className="ticket-path-row" data-state={row.state} data-current={row.current || undefined}>
      <span className="ticket-path-icon">
        <Icon name={row.icon} size={13} />
      </span>
      <span className="ticket-path-text">
        <span className="ticket-path-stage">{row.stage}</span>
        <span className="ticket-path-detail">{row.detail}</span>
      </span>
      <span className="ticket-path-model">{row.model}</span>
      {row.lock !== null ? <Lock text={row.lock} /> : <span className="ticket-path-no-lock" />}
    </div>
  );
}

/** The ticket's path: the one-line summary, then each stage. */
export function TicketPath({ view }: { view: TicketPathView }) {
  return (
    <div className="ticket-path">
      <span className="stage-choices-heading">{view.heading}</span>
      <p className="ticket-path-summary">{view.summary}</p>
      <div className="ticket-path-rows">
        {view.rows.map((row, i) => (
          <PathLine key={`${String(i)}-${row.stage}`} row={row} />
        ))}
      </div>
    </div>
  );
}
