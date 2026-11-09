// The List: the board as rows. Board and List are two layouts of the same board, so the groups are the board's
// columns in its places (Backlog, each stage in flow order, then Done, folded to one "Done · 3" line as on the
// board) and a head's chevron folds a group as the column's ⋯ folds the column. A row says what the card says: key,
// title, the status line with its dot, who, time and the one action; only what needs you has the card's coloured
// edge. Every word is Rust's (`list_view.rs`, inside `board.view`). The Board (`board/Board.tsx`) owns the drag, the
// menu and the keys: a row is pressed, clicked and dragged as a card is (`board.drag` / `board.drop`), and every group
// (and the Done line) registers its element as its column's drop target.
import { type KeyboardEvent, memo, type PointerEvent, type ReactNode, useLayoutEffect, useRef } from "react";

import { paletteInk, toneInk } from "../../cards/look";
import { Button, CodeText, PriorityChip, Tooltip } from "../../components";
import type { BoardDropTarget } from "../../generated/BoardDropTarget";
import type { CardActionKind } from "../../generated/CardActionKind";
import type { DragVerdict } from "../../generated/DragVerdict";
import type { ListDoneLine } from "../../generated/ListDoneLine";
import type { ListGroup } from "../../generated/ListGroup";
import type { ListRow } from "../../generated/ListRow";
import type { ListView } from "../../generated/ListView";
import type { ViewChange } from "../../generated/ViewChange";
import { Icon } from "../../theme/Icon";
import { rowKeys } from "./rows";
import "./list.css";

/** What the Board does for a row: the same as for a card. */
export interface ListHandlers {
  press: (key: string, e: PointerEvent) => void;
  key: (key: string, target: BoardDropTarget | null, e: KeyboardEvent) => void;
  action: (key: string, kind: CardActionKind) => void;
  menu: (key: string, at: { left: number; top: number }) => void;
}

export interface ListProps {
  list: ListView;
  selected: string | null;
  /** The row being dragged, and each group's verdict for it (`undefined` while nothing is dragged). */
  lifted: string | null;
  verdict: (target: BoardDropTarget) => DragVerdict | undefined;
  handlers: ListHandlers;
  /** A group (or the Done line), as its column's drop target, measured when a drag starts. */
  group: (id: BoardDropTarget, el: HTMLElement | null) => void;
  onView: (change: ViewChange) => void;
  /** What ends the rows, after the Done line (the board's Edit flow), scrolled with them. */
  footer?: ReactNode;
}

export interface RowProps {
  row: ListRow;
  target: BoardDropTarget | null;
  selected: boolean;
  lifted: boolean;
  focusable: boolean;
  handlers: ListHandlers;
}

/** One ticket's row: the card in one line. */
export const Row = memo(function Row({ row, target, selected, lifted, focusable, handlers }: RowProps) {
  const { card } = row;
  const key = card.key;
  // The button keeps its own clicks: the row's press (a click opens it, a drag moves it) doesn't start there.
  const stop = (e: { stopPropagation: () => void }) => {
    e.stopPropagation();
  };
  return (
    <div
      className="list-row"
      data-row={key}
      data-selected={selected || undefined}
      data-lifted={lifted || undefined}
      data-edge={card.edge ?? undefined}
      role="button"
      tabIndex={focusable ? 0 : -1}
      aria-label={`${card.key} ${card.title}`}
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
      <span
        className="list-row-edge"
        aria-hidden
        style={card.edge === null ? undefined : { background: toneInk(card.edge).fg }}
      />
      <span className="list-row-key">{card.key}</span>
      <span className="list-row-title">
        {card.priority !== null && <PriorityChip priority={card.priority} />}
        <Tooltip text={card.title}>
          <span className="list-row-title-text">{card.title}</span>
        </Tooltip>
      </span>
      <span className="list-row-status">
        {card.status !== null && (
          <>
            <span className="list-row-dot" style={{ background: paletteInk(card.status.dot) }} />
            <span className="list-row-status-text" style={{ color: toneInk(card.status.tone).fg }}>
              <CodeText text={card.status.text} />
            </span>
          </>
        )}
      </span>
      <span className="list-row-who">{card.who ?? ""}</span>
      <span className="list-row-time" data-stale={card.time?.stale || undefined}>
        {card.time?.text ?? ""}
      </span>
      <span className="list-row-action" onPointerDown={stop}>
        {card.action && (
          <Button
            label={card.action.label}
            variant={card.action.primary ? "primary" : "secondary"}
            compact
            onClick={() => {
              if (card.action) handlers.action(key, card.action.kind);
            }}
          />
        )}
      </span>
    </div>
  );
});

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
  const head = (
    <>
      <Icon name={group.folded ? "chevron-right" : "chevron-down"} size={13} />
      <span className="list-group-title">{group.title}</span>
      <span className="list-group-count">{group.count}</span>
    </>
  );
  return (
    <section
      className="list-group"
      aria-label={group.title}
      data-folded={group.folded || undefined}
      data-verdict={verdict?.kind === "home" ? undefined : verdict?.kind}
      ref={(el) => {
        element(target, el);
      }}
    >
      <div className="list-group-head">
        {toggle === null ? (
          <span className="list-group-toggle">{head}</span>
        ) : (
          <button
            type="button"
            className="list-group-toggle"
            aria-expanded={!group.folded}
            onClick={() => {
              onView(toggle);
            }}
          >
            {head}
          </button>
        )}
        {verdict?.kind === "forbidden" && <span className="list-group-refused">{verdict.title}</span>}
      </div>
      {group.empty !== null && <div className="list-group-empty">{group.empty}</div>}
      {group.entries.length > 0 && (
        <div className="list-group-rows">
          {group.entries.map(({ row }) => (
            <Row
              key={row.card.key}
              row={row}
              target={target}
              selected={row.card.key === selected}
              lifted={row.card.key === lifted}
              focusable={row.card.key === tabStop}
              handlers={handlers}
            />
          ))}
        </div>
      )}
    </section>
  );
}

/** Done folded: "Done · 3" at the end; a click opens it, and it stays Done's drop target. */
function DoneLine({
  line,
  verdict,
  element,
  onView,
}: {
  line: ListDoneLine;
  verdict: DragVerdict | undefined;
  element: ListProps["group"];
  onView: ListProps["onView"];
}) {
  return (
    <div
      className="list-done"
      data-verdict={verdict?.kind === "home" ? undefined : verdict?.kind}
      ref={(el) => {
        element(line.target, el);
      }}
    >
      <button
        type="button"
        className="list-done-button"
        aria-expanded={false}
        onClick={() => {
          onView(line.unfold);
        }}
      >
        {line.text}
      </button>
      {verdict?.kind === "forbidden" && <span className="list-group-refused">{verdict.title}</span>}
    </div>
  );
}

export function List({ list, selected, lifted, verdict, handlers, group, onView, footer }: ListProps) {
  const body = useRef<HTMLDivElement>(null);
  const keys = rowKeys(list);
  // One tab stop: the selected row, else the first.
  const tabStop = selected !== null && keys.includes(selected) ? selected : (keys[0] ?? null);
  // Switching from the columns keeps the selected ticket in view (BRD-01).
  useLayoutEffect(() => {
    if (selected === null) return;
    body.current
      ?.querySelector<HTMLElement>(`[data-row="${CSS.escape(selected)}"]`)
      ?.scrollIntoView({ block: "nearest" });
    // Only when the List opens or another ticket is selected, not on each refetch.
  }, [selected]);
  return (
    <div className="list" role="region" aria-label="List">
      <div className="list-body" ref={body}>
        {list.groups.map((g) => (
          <Group
            key={g.id}
            group={g}
            selected={selected}
            lifted={lifted}
            tabStop={tabStop}
            verdict={verdict(g.target)}
            handlers={handlers}
            element={group}
            onView={onView}
          />
        ))}
        {list.done !== null && (
          <DoneLine line={list.done} verdict={verdict(list.done.target)} element={group} onView={onView} />
        )}
        {footer}
      </div>
    </div>
  );
}
