// The List view (LST-01, LST-02; *24 · The two views* `s24-03`, by status `s24-04`, narrowed by the agent panel
// `s24-05`): the header TICKET · WHERE IN THE FLOW · WHAT'S HAPPENING · WHO, then the groups of the project's Group by,
// each with its icon, count and hint, and their rows: the coloured edge, the status icon, key, title, the track (one
// step per stage, then the place), the card's status line, who, time and the one action (or Start on a Backlog row).
// A blocked row leads with the link and reads its amber line; a ticket others wait for says "blocks 1" (DPV-01).
// What's happening takes whatever width is left, so the agent panel open narrows only it. Every word is Rust's
// (`list_view.rs`, inside `board.view`). The Board (`board/Board.tsx`) owns the drag, the menu and the keys: a row is
// pressed, clicked and dragged as a card is (`board.drag` / `board.drop`), and a group that is a drop target (Group by
// Stage) registers its element so a row dragged onto Ready queues it with the drop zone's words; Done's "show" sends
// its fold (`board.set_view`). Grouped by Epic (EPL-02), each epic is a parent row (its fold chevron, key or External
// ID, name, track, "1 of 5 merged · Wishlist lead is re-planning" and its lead) with its children's rows under it; a
// click on the parent opens the epic.
import { type KeyboardEvent, memo, type PointerEvent, useLayoutEffect, useRef } from "react";

import { paletteInk, toneInk } from "../../cards/look";
import { Button, PriorityChip, ToolAvatar, Tooltip } from "../../components";
import { LeadStatus } from "../../components/LeadStatus";
import type { BoardDropTarget } from "../../generated/BoardDropTarget";
import type { CardActionKind } from "../../generated/CardActionKind";
import type { DragVerdict } from "../../generated/DragVerdict";
import type { ListGroup } from "../../generated/ListGroup";
import type { ListParent } from "../../generated/ListParent";
import type { ListRow } from "../../generated/ListRow";
import type { ListTrack } from "../../generated/ListTrack";
import type { ListView } from "../../generated/ListView";
import type { ViewChange } from "../../generated/ViewChange";
import { Icon } from "../../theme/Icon";
import { rowsOf } from "./rows";
import "./list.css";

/** What the Board does for a row: the same as for a card. */
export interface ListHandlers {
  press: (key: string, e: PointerEvent) => void;
  key: (key: string, target: BoardDropTarget | null, e: KeyboardEvent) => void;
  action: (key: string, kind: CardActionKind) => void;
  /** Start on a Backlog row: the drop ⌘D sends. */
  start: (key: string, target: BoardDropTarget) => void;
  menu: (key: string, at: { left: number; top: number }) => void;
  /** A click (or ↵) on an epic's parent row: opens the epic. */
  open: (key: string) => void;
}

export interface ListProps {
  list: ListView;
  selected: string | null;
  /** The row being dragged, and each group's verdict for it (`undefined` while nothing is dragged). */
  lifted: string | null;
  verdict: (target: BoardDropTarget) => DragVerdict | undefined;
  handlers: ListHandlers;
  /** A group that is a drop target, measured when a drag starts. */
  group: (id: BoardDropTarget, el: HTMLElement | null) => void;
  onView: (change: ViewChange) => void;
}

/** A row's track: a step per stage, then its place (the epic sheet's Tickets tab draws it too). */
export function Track({ track }: { track: ListTrack }) {
  return (
    <span className="list-track">
      <span className="list-track-steps">
        {track.steps.map((step, i) => (
          <Tooltip key={`${String(i)}:${step.name}`} text={step.name}>
            <span
              className="list-track-step"
              style={step.fill === null ? undefined : { background: paletteInk(step.fill) }}
            />
          </Tooltip>
        ))}
      </span>
      <span className="list-track-label">{track.label}</span>
    </span>
  );
}

export interface RowProps {
  row: ListRow;
  target: BoardDropTarget | null;
  selected: boolean;
  lifted: boolean;
  focusable: boolean;
  handlers: ListHandlers;
}

/** One ticket's row (LST-01); the epic sheet's Tickets tab draws its children with it (SHT-07). */
export const Row = memo(function Row({ row, target, selected, lifted, focusable, handlers }: RowProps) {
  const { card } = row;
  const key = card.key;
  // The buttons keep their own clicks: the row's press (a click opens it, a drag moves it) doesn't start there.
  const stop = (e: { stopPropagation: () => void }) => {
    e.stopPropagation();
  };
  return (
    <div
      className="list-row"
      data-row={key}
      data-selected={selected || undefined}
      data-lifted={lifted || undefined}
      data-dimmed={row.dim || undefined}
      data-edge={card.edge ?? undefined}
      style={card.edge === null ? undefined : { borderLeftColor: toneInk(card.edge).fg }}
      role="button"
      tabIndex={focusable ? 0 : -1}
      aria-label={`${card.key_label} ${card.title}`}
      aria-pressed={selected}
      onPointerDown={(e) => {
        handlers.press(key, e);
      }}
      onKeyDown={(e) => {
        handlers.key(key, target, e);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        handlers.menu(key, { left: e.clientX, top: e.clientY });
      }}
    >
      <span className="list-row-icon" style={{ color: paletteInk(row.palette) }}>
        <Icon name={row.icon} size={14} />
      </span>
      <span className="list-row-key">{card.key_label}</span>
      <span className="list-row-title">
        {card.epic !== null && (
          <span className="list-row-epic">
            <Icon name="layers" size={11} />
            <span>{card.epic}</span>
          </span>
        )}
        {card.blocks === null && card.priority === null ? (
          <Tooltip text={card.title}>
            <span className="list-row-title-text">{card.title}</span>
          </Tooltip>
        ) : (
          <span className="list-row-title-line">
            {/* The card's priority chip before the title (#39): P1, P2, P4; Normal shows none. */}
            {card.priority !== null && <PriorityChip priority={card.priority} />}
            <Tooltip text={card.title}>
              <span className="list-row-title-text">{card.title}</span>
            </Tooltip>
            {card.blocks !== null && (
              <span className="list-row-blocks">
                <Icon name="link" size={11} />
                <span>{card.blocks.tag}</span>
              </span>
            )}
          </span>
        )}
      </span>
      <Track track={row.track} />
      <span
        className="list-row-status"
        style={card.status === null ? undefined : { color: toneInk(card.status.tone).fg }}
      >
        {card.status?.text ?? ""}
      </span>
      <span className="list-row-who">
        {card.avatar && <ToolAvatar avatar={card.avatar} />}
        <span className="list-row-who-name">{card.who ?? ""}</span>
      </span>
      <span className="list-row-time" data-stale={card.time?.stale || undefined}>
        {card.time?.text ?? ""}
      </span>
      <span className="list-row-action" onPointerDown={stop}>
        {card.action ? (
          <Button
            label={card.action.label}
            variant={card.action.primary ? "primary" : "secondary"}
            compact
            onClick={() => {
              if (card.action) handlers.action(key, card.action.kind);
            }}
          />
        ) : (
          row.start && (
            <Button
              label={row.start.label}
              variant="secondary"
              compact
              onClick={() => {
                if (row.start) handlers.start(key, row.start.target);
              }}
            />
          )
        )}
      </span>
    </div>
  );
});

interface ParentProps {
  parent: ListParent;
  handlers: ListHandlers;
  onView: ListProps["onView"];
}

/** An epic's parent row (EPL-02): the fold chevron in the icon's place, then as a row. */
function Parent({ parent, handlers, onView }: ParentProps) {
  const { row } = parent;
  const stop = (e: { stopPropagation: () => void }) => {
    e.stopPropagation();
  };
  return (
    <div
      className="list-row list-parent"
      data-edge={row.needs_you ? "attention" : undefined}
      style={row.needs_you ? { borderLeftColor: toneInk("attention").fg } : undefined}
      role="button"
      tabIndex={-1}
      aria-label={`${row.key} ${row.name}`}
      onClick={() => {
        handlers.open(row.key);
      }}
      onKeyDown={(e) => {
        if (e.target === e.currentTarget && e.key === "Enter") handlers.open(row.key);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        handlers.menu(row.key, { left: e.clientX, top: e.clientY });
      }}
    >
      <Tooltip text={parent.fold_label}>
        <button
          type="button"
          className="list-parent-fold"
          aria-label={parent.fold_label}
          aria-expanded={!parent.folded}
          onClick={(e) => {
            stop(e);
            onView(parent.fold);
          }}
        >
          <Icon name={parent.folded ? "chevron-right" : "chevron-down"} size={14} />
        </button>
      </Tooltip>
      <span className="list-row-key">{row.external_id ?? row.key}</span>
      <span className="list-row-title">
        <span className="list-row-epic">
          <Icon name="layers" size={11} />
        </span>
        <Tooltip text={row.name}>
          <span className="list-row-title-text">{row.name}</span>
        </Tooltip>
      </span>
      <Track track={parent.track} />
      <span className="list-row-status">{row.line}</span>
      {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- only keeps the click from opening the epic; the lead's status is the control */}
      <span className="list-row-who" onClick={stop}>
        {row.lead && <LeadStatus lead={row.lead} form="short" />}
      </span>
      <span />
      <span />
    </div>
  );
}

interface GroupProps {
  group: ListGroup;
  selected: string | null;
  lifted: string | null;
  tabStop: string | null;
  verdict: DragVerdict | undefined;
  handlers: ListHandlers;
  element: ListProps["group"];
  onView: ListProps["onView"];
}

function Group({ group, selected, lifted, tabStop, verdict, handlers, element, onView }: GroupProps) {
  const { target, toggle } = group;
  return (
    <section
      className="list-group"
      aria-label={group.title}
      data-verdict={verdict?.kind === "home" ? undefined : verdict?.kind}
      ref={(el) => {
        if (target !== null) element(target, el);
      }}
    >
      <div className="list-group-head">
        <Icon name={group.icon} size={13} />
        <span
          className="list-group-title"
          style={group.tone === null ? undefined : { color: toneInk(group.tone).fg }}
        >
          {group.title}
        </span>
        <span className="list-group-count">{group.count}</span>
        {group.hint !== null && (
          <>
            <span className="list-group-dot" aria-hidden>
              ·
            </span>
            {toggle === null ? (
              <span className="list-group-hint">{group.hint}</span>
            ) : (
              <button
                type="button"
                className="list-group-hint list-group-toggle"
                aria-expanded={!group.folded}
                onClick={() => {
                  onView(toggle);
                }}
              >
                {group.hint}
              </button>
            )}
          </>
        )}
        {verdict?.kind === "forbidden" && <span className="list-group-refused">{verdict.title}</span>}
      </div>
      {group.empty !== null && <div className="list-group-empty">{group.empty}</div>}
      {group.entries.map((entry) => {
        const draw = (row: ListRow) => (
          <Row
            key={row.card.key}
            row={row}
            target={target}
            selected={row.card.key === selected}
            lifted={row.card.key === lifted}
            focusable={row.card.key === tabStop}
            handlers={handlers}
          />
        );
        if (entry.kind === "ticket") return draw(entry.row);
        return (
          <div key={entry.parent.row.key} className="list-family">
            <Parent parent={entry.parent} handlers={handlers} onView={onView} />
            {entry.parent.children.map(draw)}
          </div>
        );
      })}
    </section>
  );
}

export function List({ list, selected, lifted, verdict, handlers, group, onView }: ListProps) {
  const body = useRef<HTMLDivElement>(null);
  const keys = list.groups.flatMap((g) => rowsOf(g).map((r) => r.card.key));
  // One tab stop: the selected row, else the first.
  const tabStop = selected !== null && keys.includes(selected) ? selected : (keys[0] ?? null);
  // Switching from the Board keeps the selected ticket in view (BRD-01).
  useLayoutEffect(() => {
    if (selected === null) return;
    body.current
      ?.querySelector<HTMLElement>(`[data-row="${CSS.escape(selected)}"]`)
      ?.scrollIntoView({ block: "nearest" });
    // Only when the List opens or another ticket is selected, not on each refetch.
  }, [selected]);
  return (
    <div className="list" role="region" aria-label="List">
      <div className="list-header" aria-hidden>
        <span className="list-header-ticket">{list.header.ticket}</span>
        <span>{list.header.flow}</span>
        <span>{list.header.happening}</span>
        <span>{list.header.who}</span>
      </div>
      <div className="list-body" ref={body}>
        {list.groups.map((g) => (
          <Group
            key={g.id}
            group={g}
            selected={selected}
            lifted={lifted}
            tabStop={tabStop}
            verdict={g.target === null ? undefined : verdict(g.target)}
            handlers={handlers}
            element={group}
            onView={onView}
          />
        ))}
      </div>
    </div>
  );
}
