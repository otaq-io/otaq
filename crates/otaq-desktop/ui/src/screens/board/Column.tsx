// One board column (BRD-02, BRD-04; *24 · The two views*): the header with icon, title and count (Backlog's plus,
// the ⋯ menu on hover with Fold this column), Ready for Dev's hint, the cards (Dev: the floated cards and the "N more ·
// working" fold; Review: its two sections, below), the empty hint, Backlog's "Add ticket" row, and the "7 above · 6
// need you" chip of a column scrolled down (*Very full column*). A folded column is a 52 px rail with its icon,
// count, a dot when something in it needs you, and its name (`s26-10`); a click unfolds it. Done folded is the board's
// "Done 4 →" line, under the last column. Which of these a column is comes from its `kind` (STG-08), never its id.
// Folds are view preferences kept by the service (SHB-05): the column asks the board to change them (`onView`). The
// header (FLW-12): one 13 px icon by the stage's kind; the stages folded into an agent column after its count ("Dev 3
// · Validating 1"); hovering the title shows who runs it (agent, model, options).
// The Review column (*Row · Review column (0086 · V)*, d0087) holds "READY FOR REVIEW" and "APPROVED": each label folds
// its section (the count stays), an empty section says so, and Approved (the section with a `target` of its own) is a
// drop target of its own. Everything it says is `BoardColumn`'s; while a card is dragged over it, it takes the
// verdict's outline. Under the chain highlight (T-0229) its cards take their chain look, and the above chip, Dev's
// fold and the rail turn accent for members they hide.
import {
  type KeyboardEvent,
  memo,
  type PointerEvent,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { useSelection } from "../../app/selection";
import { type ChainState, RESTING, TicketCard } from "../../cards";
import { AnchoredMenu, type Anchor, Tooltip } from "../../components";
import type { BoardCard } from "../../generated/BoardCard";
import type { BoardColumn } from "../../generated/BoardColumn";
import type { BoardColumnId } from "../../generated/BoardColumnId";
import type { BoardColumnKind } from "../../generated/BoardColumnKind";
import type { BoardDropTarget } from "../../generated/BoardDropTarget";
import type { BoardDoneLine } from "../../generated/BoardDoneLine";
import type { BoardReviewSection } from "../../generated/BoardReviewSection";
import type { BoardScrollChip } from "../../generated/BoardScrollChip";
import type { CardActionKind } from "../../generated/CardActionKind";
import type { ChainView } from "../../generated/ChainView";
import type { DragVerdict } from "../../generated/DragVerdict";
import type { ViewChange } from "../../generated/ViewChange";
import { Icon } from "../../theme/Icon";
import { columnEditorOpen, openFlowEditor } from "../flow_editor/logic";
import { chainStateOf, membersIn } from "./chain";
import { type AboveChip, aboveChip } from "./drag";

/** What a card does on the board; the same functions for every card, so the card lists don't redraw. */
export interface CardHandlers {
  press: (key: string, column: BoardColumnId, e: PointerEvent) => void;
  key: (key: string, column: BoardColumnId, e: KeyboardEvent) => void;
  open: (key: string) => void;
  action: (key: string, kind: CardActionKind) => void;
  /** The card's right-click menu, at the pointer (or under the card, from the keyboard). */
  menu: (key: string, at: { left: number; top: number }) => void;
  /** The pointer came onto (`true`) or left (`false`) the card: its chain lights up. */
  hover: (key: string, over: boolean) => void;
}

/** The column icon's colour (*03*: each column's icon in its status colour), by what the column holds. */
const ICON_INK: Record<BoardColumnKind, string> = {
  backlog: "var(--st-queued)",
  ready_for_dev: "var(--st-queued)",
  agent: "var(--st-working)",
  human_check: "var(--st-review)",
  delivery: "var(--st-approved)",
  done: "var(--st-done)",
};

interface CardItemProps {
  card: BoardCard;
  column: BoardColumnId;
  selected: boolean;
  chain: ChainState;
  lifted: boolean;
  focusable: boolean;
  handlers: CardHandlers;
}

const CardItem = memo(function CardItem({
  card,
  column,
  selected,
  chain,
  lifted,
  focusable,
  handlers,
}: CardItemProps) {
  const key = card.card.key;
  const state = selected || chain !== "none" ? { ...RESTING, selected, chain } : RESTING;
  return (
    <div
      className="board-card"
      data-card={key}
      data-needs-you={card.needs_you || undefined}
      data-lifted={lifted || undefined}
      role="button"
      tabIndex={focusable ? 0 : -1}
      aria-label={`${key} ${card.card.title}`}
      aria-pressed={selected}
      onPointerDown={(e) => {
        handlers.press(key, column, e);
      }}
      onKeyDown={(e) => {
        handlers.key(key, column, e);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        handlers.menu(key, { left: e.clientX, top: e.clientY });
      }}
      onPointerEnter={() => {
        handlers.hover(key, true);
      }}
      onPointerLeave={() => {
        handlers.hover(key, false);
      }}
    >
      <TicketCard
        card={card.card}
        state={state}
        onAction={(kind) => {
          handlers.action(key, kind);
        }}
        onOpenSheet={() => {
          handlers.open(key);
        }}
      />
    </div>
  );
});

interface CardListProps {
  cards: readonly BoardCard[];
  column: BoardColumnId;
  selected: string | null;
  chain: ChainView | null;
  lifted: string | null;
  /** The card that is the column's tab stop (the selected one, else the first). */
  tabStop: string | null;
  handlers: CardHandlers;
}

export const CardList = memo(function CardList({
  cards,
  column,
  selected,
  chain,
  lifted,
  tabStop,
  handlers,
}: CardListProps) {
  return (
    <>
      {cards.map((card) => (
        <CardItem
          key={card.card.key}
          card={card}
          column={column}
          selected={card.card.key === selected}
          chain={chainStateOf(chain, card.card.key)}
          lifted={card.card.key === lifted}
          focusable={card.card.key === tabStop}
          handlers={handlers}
        />
      ))}
    </>
  );
});

/** The column a card is in, by its key; `null` for a key that is no card. */
export function columnOfCard(columns: readonly BoardColumn[], key: string): BoardColumnId | null {
  return columns.find((c) => cardsOf(c).some((card) => card.card.key === key))?.id ?? null;
}

/** Every card of a column in board order, the folded ones included. */
export function cardsOf(column: BoardColumn): BoardCard[] {
  return column.dev
    ? [...column.dev.top, ...column.dev.working]
    : [...column.cards, ...column.sections.flatMap((s) => s.cards)];
}

/** The cards the keyboard walks: every card but those in a folded Review section. */
export function walkOf(column: BoardColumn, foldedSections: readonly string[] = []): BoardCard[] {
  const folded = new Set(
    column.sections
      .filter((s) => foldedSections.includes(s.id))
      .flatMap((s) => s.cards.map((c) => c.card.key)),
  );
  return cardsOf(column).filter((c) => !folded.has(c.card.key));
}

export interface ColumnProps {
  column: BoardColumn;
  chipWords: BoardScrollChip;
  /** The column's look under a dragged card; `undefined` while nothing is dragged over it. */
  verdict?: DragVerdict | undefined;
  /** A card is being dragged (anywhere). */
  dragging: boolean;
  selected: string | null;
  /** The highlighted chain, if any (T-0229). */
  chain?: ChainView | null;
  lifted: string | null;
  handlers: CardHandlers;
  /** The column's own empty hint and "Add ticket" row; off while the empty board's card covers the board. */
  showEmptyHint?: boolean;
  /** Where this column was scrolled when the project was last shown. */
  scrollTop: number;
  onScroll: (column: BoardColumnId, top: number) => void;
  /** A change of the view preferences: a fold from the ⋯ menu or the rail, Dev's fold, a Review section. */
  onView: (change: ViewChange) => void;
  onAdd: () => void;
  /** Review: the sections folded by the user (a view preference, SHB-05). */
  foldedSections?: readonly string[];
  /** The last open column: Done's "Done 4 →" line goes under its cards while Done is folded. */
  doneLine?: BoardDoneLine | null;
  /** Review, while a card is dragged: the verdict of its section with a target of its own (Approved), and whether
   * the card is over it. */
  approved?: { verdict: DragVerdict | undefined; over: boolean } | undefined;
  /** The column's element (and its sections' with a target of their own: Approved), measured when a drag starts. */
  element: (target: BoardDropTarget, el: HTMLElement | null) => void;
}

export function verdictLook(verdict: DragVerdict | undefined): string | undefined {
  return verdict === undefined || verdict.kind === "home" ? undefined : verdict.kind;
}

/** How a Review section looks under the dragged card: lit (or red) only while the card is over it. */
export function sectionLook(
  drag: { verdict: DragVerdict | undefined; over: boolean } | undefined,
): string | undefined {
  return drag?.over ? verdictLook(drag.verdict) : undefined;
}

interface ReviewSectionProps extends Omit<CardListProps, "cards"> {
  section: BoardReviewSection;
  folded: boolean;
  /** A section with a target of its own (Approved), while a card is dragged. */
  drag: { verdict: DragVerdict | undefined; over: boolean } | undefined;
  onToggle: () => void;
  element: ColumnProps["element"];
}

function ReviewSection({ section, folded, drag, onToggle, element, ...list }: ReviewSectionProps) {
  const look = sectionLook(drag);
  const hint = look === "allowed" && drag?.verdict?.kind === "allowed" ? drag.verdict.hint : null;
  // A Ready for Review card is the one Approved takes: while it is dragged, an empty Approved says how.
  const empty =
    drag?.verdict?.kind === "allowed" && section.drag_hint !== null ? section.drag_hint : section.empty;
  return (
    <div
      className="board-review-section"
      data-section={section.id}
      data-verdict={look}
      ref={
        section.target !== null
          ? (el) => {
              if (section.target !== null) element(section.target, el);
            }
          : undefined
      }
    >
      <Tooltip text={folded ? section.unfold : section.fold}>
        <button type="button" className="board-review-label" aria-expanded={!folded} onClick={onToggle}>
          <Icon name={folded ? "chevron-right" : "chevron-down"} size={11} />
          <span className="board-review-title">{section.title}</span>
          <span className="board-review-count">{section.count}</span>
        </button>
      </Tooltip>
      {hint !== null && <div className="board-review-hint">{hint}</div>}
      {!folded && (
        <>
          <CardList cards={section.cards} {...list} />
          {section.cards.length === 0 && <div className="board-review-empty">{empty}</div>}
          {section.hidden !== null && <div className="board-hidden">{section.hidden}</div>}
        </>
      )}
    </div>
  );
}

/** The icon's colour: muted on a flow that isn't the default's shape (FLW-12), else its kind's status colour. */
export function iconInk(column: Pick<BoardColumn, "kind" | "quiet_icon">): string {
  return column.quiet_icon ? "var(--text-muted)" : ICON_INK[column.kind];
}

/** A folded column (BRD-02, `s26-10`): a 52 px rail with its icon, count, a needs-you dot and its name; a click
 * unfolds it. It stays a drop target. */
export function Rail({
  column,
  members,
  onView,
  element,
}: Pick<ColumnProps, "column" | "element" | "onView"> & { members: string[] }) {
  const rail = column.rail;
  if (rail === null) return null;
  return (
    <Tooltip text={rail.tooltip} detail={[rail.hint]}>
      <button
        type="button"
        className="board-rail"
        data-column={column.id}
        data-chain={members.length > 0 || undefined}
        ref={(el) => {
          element(column.id, el);
        }}
        aria-label={rail.tooltip}
        onClick={() => {
          onView(rail.unfold);
        }}
      >
        <span style={{ color: iconInk(column) }}>
          <Icon name={column.icon} size={13} />
        </span>
        <span className="board-rail-count">{column.count}</span>
        {rail.dot && <span className="board-rail-dot" aria-hidden />}
        <span className="board-rail-title">{column.title}</span>
      </button>
    </Tooltip>
  );
}

/** Done folded (BRD-02, `s24-03`): "Done 4 →" under the last column; a click opens Done. It stays Done's drop target. */
export function DoneLink({
  line,
  onView,
  element,
}: { line: BoardDoneLine } & Pick<ColumnProps, "onView" | "element">) {
  const done = line.unfold.kind === "fold_column" ? line.unfold.column : "done";
  return (
    <button
      type="button"
      className="board-done-line"
      data-column={done}
      ref={(el) => {
        element(done, el);
      }}
      onClick={() => {
        onView(line.unfold);
      }}
    >
      {line.text}
    </button>
  );
}

/**
 * The column's ⋯ (BRD-04, FED-09; `s26-08`), shown on hover: Edit <stage>…, Add a stage after <stage>, Fold this
 * column, Change the whole flow…. The flow's items open the flow editor over the board on the shown project.
 */
export function ColumnMenu({ column, onView }: Pick<ColumnProps, "column" | "onView">) {
  const { project } = useSelection();
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  if (column.menu.length === 0) return null;
  return (
    <>
      <Tooltip text={column.menu_label}>
        <button
          type="button"
          className="board-column-button board-column-menu"
          aria-label={column.menu_label}
          aria-haspopup="menu"
          aria-expanded={anchor !== null}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            setAnchor({ left: r.left, top: r.bottom + 4, width: r.width, height: 0 });
          }}
        >
          <Icon name="ellipsis" size={14} />
        </button>
      </Tooltip>
      {anchor && (
        <AnchoredMenu
          anchor={anchor}
          label={column.menu_label}
          items={column.menu.map((m) => m.item)}
          width={270}
          onChoose={(id) => {
            setAnchor(null);
            const chosen = column.menu.find((m) => m.item.id === id);
            if (chosen?.change) onView(chosen.change);
            if (chosen?.editor && project !== null) openFlowEditor(columnEditorOpen(project, chosen.editor));
          }}
          onClose={() => {
            setAnchor(null);
          }}
        />
      )}
    </>
  );
}

/** The header's icon, title (with who runs it on hover), count and folded stages. */
export function HeaderTitle({ column }: { column: BoardColumn }) {
  const title = <span className="board-column-title">{column.title}</span>;
  return (
    <>
      <span className="board-column-icon" style={{ color: iconInk(column) }}>
        <Icon name={column.icon} size={13} />
      </span>
      {column.tooltip !== null ? (
        <Tooltip text={column.tooltip.title} detail={column.tooltip.detail}>
          {title}
        </Tooltip>
      ) : (
        title
      )}
      <span className="board-column-count">{column.count}</span>
      {column.folded.map((f) => (
        <span key={f.tag} className="board-column-folded">
          <span className="board-column-count">·</span>
          <span className="board-column-folded-tag">{f.tag}</span>
          <span className="board-column-count">{f.count}</span>
        </span>
      ))}
    </>
  );
}

export function Column({
  column,
  chipWords,
  verdict,
  dragging,
  selected,
  chain = null,
  lifted,
  handlers,
  showEmptyHint = true,
  scrollTop,
  onScroll,
  onView,
  onAdd,
  foldedSections = [],
  doneLine = null,
  approved,
  element,
}: ColumnProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const [chip, setChip] = useState<AboveChip | null>(null);
  const [aboveKeys, setAboveKeys] = useState<string[]>([]);
  const all = cardsOf(column);
  const walk = walkOf(column, foldedSections);
  const tabStop = walk.some((c) => c.card.key === selected) ? selected : (walk[0]?.card.key ?? null);

  // The chip counts the cards scrolled out above the view, and how many of them need the user.
  const measure = useCallback(() => {
    const box = scroller.current;
    if (!box) return;
    const above = [...box.querySelectorAll<HTMLElement>("[data-card]")].filter(
      (el) => el.offsetTop + el.offsetHeight <= box.scrollTop,
    );
    setChip(
      aboveChip(
        chipWords,
        above.map((el) => el.dataset.needsYou !== undefined),
      ),
    );
    setAboveKeys((old) => {
      const next = above.map((el) => el.dataset.card ?? "");
      return old.length === next.length && old.every((k, i) => k === next[i]) ? old : next;
    });
  }, [chipWords]);

  // Back where the user left it when the project is shown again.
  useLayoutEffect(() => {
    const box = scroller.current;
    if (box && box.scrollTop !== scrollTop) box.scrollTop = scrollTop;
    measure();
    // Only when the column (project) changes, not on each scroll the column reports itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- scrollTop is read once per column shown
  }, [column.id, measure]);

  if (column.collapsed)
    return (
      <Rail
        column={column}
        members={membersIn(
          chain,
          all.map((c) => c.card.key),
        )}
        onView={onView}
        element={element}
      />
    );

  const empty = all.length === 0;
  return (
    <section
      className="board-column"
      ref={(el) => {
        element(column.id, el);
      }}
      aria-label={column.title}
      data-column={column.id}
      data-kind={column.kind}
      data-verdict={dragging ? verdictLook(verdict) : undefined}
    >
      <header className="board-column-header">
        <HeaderTitle column={column} />
        <span className="board-spacer" />
        {column.add !== null && (
          <Tooltip text={column.add}>
            <button type="button" className="board-column-button" aria-label={column.add} onClick={onAdd}>
              <Icon name="plus" size={14} />
            </button>
          </Tooltip>
        )}
        <ColumnMenu column={column} onView={onView} />
      </header>
      {column.hint !== null && <div className="board-column-hint">{column.hint}</div>}
      {chip !== null && (
        <div className="board-above">
          <button
            type="button"
            className="board-chip"
            data-attention={chip.needYou || undefined}
            data-chain={membersIn(chain, aboveKeys).length > 0 || undefined}
            onClick={() => {
              scroller.current?.scrollTo({ top: 0, behavior: "smooth" });
            }}
          >
            <Icon name="chevron-up" size={12} />
            <span>{chip.text}</span>
          </button>
        </div>
      )}
      <div
        className="board-column-cards"
        ref={scroller}
        onScroll={(e) => {
          onScroll(column.id, e.currentTarget.scrollTop);
          measure();
        }}
      >
        {column.dev ? (
          <>
            <CardList
              cards={column.dev.top}
              column={column.id}
              selected={selected}
              chain={chain}
              lifted={lifted}
              tabStop={tabStop}
              handlers={handlers}
            />
            <CardList
              cards={column.dev.working}
              column={column.id}
              selected={selected}
              chain={chain}
              lifted={lifted}
              tabStop={tabStop}
              handlers={handlers}
            />
            {column.dev.fold !== null && (
              <div className="board-fold">
                <button
                  type="button"
                  className="board-chip"
                  data-chain={membersIn(chain, column.dev.folded).length > 0 || undefined}
                  onClick={() => {
                    onView({ kind: "dev_unfolded", on: true });
                  }}
                >
                  <span>{column.dev.fold}</span>
                  <Icon name="chevron-down" size={12} />
                </button>
              </div>
            )}
          </>
        ) : column.sections.length > 0 ? (
          column.sections.map((section) => (
            <ReviewSection
              key={section.id}
              section={section}
              folded={foldedSections.includes(section.id)}
              drag={section.target !== null ? approved : undefined}
              onToggle={() => {
                onView({
                  kind: "fold_section",
                  section: section.id,
                  folded: !foldedSections.includes(section.id),
                });
              }}
              element={element}
              column={column.id}
              selected={selected}
              chain={chain}
              lifted={lifted}
              tabStop={tabStop}
              handlers={handlers}
            />
          ))
        ) : (
          <CardList
            cards={cardsOf(column)}
            column={column.id}
            selected={selected}
            chain={chain}
            lifted={lifted}
            tabStop={tabStop}
            handlers={handlers}
          />
        )}
        {empty && showEmptyHint && column.sections.length === 0 && (
          <div className="board-empty-column">
            <Icon name={column.icon} size={18} />
            <span>{column.empty_hint}</span>
          </div>
        )}
        {column.hidden !== null && <div className="board-hidden">{column.hidden}</div>}
        {column.add !== null && showEmptyHint && (
          <button type="button" className="board-add" onClick={onAdd}>
            <Icon name="plus" size={13} />
            <span>{column.add}</span>
          </button>
        )}
        {doneLine !== null && <DoneLink line={doneLine} onView={onView} element={element} />}
      </div>
    </section>
  );
}
