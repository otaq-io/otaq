// The board's toolbar, in every view (BRD-01, FIL-01; *24 · The two views*, `s28-03`): Board · List · Files first
// (`g b`, `g l`, `g f`), then the List's Group by; on the right the Board's
// Dependencies On hover · Always (DPV-02, `s24-09`), the Fold empty columns (or Hide empty stages) switch, All tickets
// and New ticket. On Files: its subtitle after a divider and the Live dot on the right (FIL-10). Every word and every
// change it sends is Rust's (`BoardToolbar`, `view_prefs.rs`); a click sends the option's change (`board.set_view`)
// and the board asks for its view again.
import type { KeyboardEvent } from "react";

import { mainWidth } from "../../app/layout";
import { useStore } from "../../app/store";
import { Tooltip } from "../../components";
import type { BoardToolbar } from "../../generated/BoardToolbar";
import type { ToolbarGroup } from "../../generated/ToolbarGroup";
import type { ToolbarOption } from "../../generated/ToolbarOption";
import type { ViewChange } from "../../generated/ViewChange";
import { Icon } from "../../theme/Icon";

export interface ToolbarProps {
  toolbar: BoardToolbar;
  onView: (change: ViewChange) => void;
  onAllTickets: () => void;
  onNewTicket: () => void;
}

/** ←/→ on a segment: the next one, wrapping, chosen at once (a radio group). */
export function stepSegment(key: string, selected: number, count: number): number | null {
  if (count === 0) return null;
  if (key === "ArrowRight" || key === "ArrowDown") return (selected + 1) % count;
  if (key === "ArrowLeft" || key === "ArrowUp") return (selected - 1 + count) % count;
  return null;
}

function Segment({ option, onView }: { option: ToolbarOption; onView: ToolbarProps["onView"] }) {
  const button = (
    <button
      type="button"
      role="radio"
      className="board-segment"
      aria-checked={option.selected}
      tabIndex={option.selected ? 0 : -1}
      onClick={() => {
        if (!option.selected) onView(option.change);
      }}
    >
      {option.icon !== null && <Icon name={option.icon} size={14} />}
      <span>{option.label}</span>
    </button>
  );
  return option.tooltip === null ? (
    button
  ) : (
    <Tooltip text={option.tooltip} {...(option.keys === null ? {} : { keys: option.keys })}>
      {button}
    </Tooltip>
  );
}

function Segments({
  group,
  name,
  onView,
}: {
  group: ToolbarGroup;
  name: string;
  onView: ToolbarProps["onView"];
}) {
  const selected = Math.max(
    0,
    group.options.findIndex((o) => o.selected),
  );
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const next = stepSegment(e.key, selected, group.options.length);
    const option = next === null ? undefined : group.options[next];
    if (option === undefined) return;
    e.preventDefault();
    onView(option.change);
  };
  return (
    <>
      {group.icon !== null && (
        <span className="board-toolbar-label-icon" aria-hidden>
          <Icon name={group.icon} size={13} />
        </span>
      )}
      {group.label !== null && <span className="board-toolbar-label">{group.label}</span>}
      {/* eslint-disable-next-line jsx-a11y/interactive-supports-focus -- its radios take the focus */}
      <div
        className="board-segments"
        role="radiogroup"
        aria-label={group.label ?? name}
        onKeyDown={onKeyDown}
      >
        {group.options.map((option) => (
          <Segment key={option.label} option={option} onView={onView} />
        ))}
      </div>
    </>
  );
}

/** Fold empty columns (Board) or Hide empty stages (List). */
function Switch({ option, onView }: { option: ToolbarOption; onView: ToolbarProps["onView"] }) {
  return (
    <button
      type="button"
      role="switch"
      className="board-chip-button"
      aria-checked={option.selected}
      onClick={() => {
        onView(option.change);
      }}
    >
      {option.icon !== null && <Icon name={option.icon} size={12} />}
      <span>{option.label}</span>
    </button>
  );
}

/** The toolbar's 16 px sides: it is 760 px or less (`s28-08`) in a main area this wide or less. */
const NARROW_MAIN = 760 + 2 * 16;

export function Toolbar({ toolbar, onView, onAllTickets, onNewTicket }: ToolbarProps) {
  const choices = toolbar.group_by;
  const main = useStore(mainWidth);
  return (
    <div
      className="board-toolbar"
      role="toolbar"
      aria-label="Board"
      data-narrow={(main > 0 && main <= NARROW_MAIN) || undefined}
    >
      <Segments group={toolbar.views} name="View" onView={onView} />
      {choices && (
        <>
          <span className="board-toolbar-divider" aria-hidden />
          <Segments group={choices} name={choices.label ?? ""} onView={onView} />
        </>
      )}
      {toolbar.subtitle !== null && (
        <>
          <span className="board-toolbar-divider board-toolbar-subtitle-divider" aria-hidden />
          <span className="board-toolbar-subtitle">{toolbar.subtitle}</span>
        </>
      )}
      <span className="board-toolbar-spacer" />
      {toolbar.live !== null && (
        <span className="board-toolbar-live">
          <span className="board-toolbar-live-dot" aria-hidden />
          <span>{toolbar.live}</span>
        </span>
      )}
      {toolbar.dependencies && (
        <Segments group={toolbar.dependencies} name={toolbar.dependencies.label ?? ""} onView={onView} />
      )}
      {toolbar.switch && <Switch option={toolbar.switch} onView={onView} />}
      {toolbar.all_tickets !== null && (
        <button type="button" className="board-chip-button" onClick={onAllTickets}>
          <span>{toolbar.all_tickets}</span>
          <Icon name="chevron-down" size={12} />
        </button>
      )}
      {toolbar.new_ticket !== null && (
        <Tooltip
          text={toolbar.new_ticket}
          {...(toolbar.new_ticket_keys === null ? {} : { keys: toolbar.new_ticket_keys })}
        >
          <button type="button" className="board-new-ticket" onClick={onNewTicket}>
            <Icon name="plus" size={13} />
            <span>{toolbar.new_ticket}</span>
          </button>
        </Tooltip>
      )}
    </div>
  );
}
