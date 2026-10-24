// The one-line ticket card (BRD-03; *24 · The two views*): the key (with its epic's name on the same row for an
// epic child) and its priority chip (P1, P2, P4), the title on two lines (the full title in a tooltip), one status line in words with a dot in the
// state's colour (a blocked ticket's in amber after a link, DPV-01), a muted "Blocks SHOP-20" when others wait for
// it, then who (tool mark and agent name) and when, and the card's one action. A card that needs the user
// has a coloured edge (turquoise, the review colour for your check, red for a closed parent). The pill, the model,
// the tokens and the answer options are the ticket's sheet's. Everything it says comes from a `TicketCardView`
// (`card_page.rs`); the board (or the gallery) decides the moment of interaction: hovered, selected, chain, dragging
// (*Interaction states*). Resting on the key row shows the full status and what the card has no room for; the quick
// actions (open terminal, open panel) show on hover.
import { type CSSProperties, type ReactNode, type RefObject, useEffect, useRef, useState } from "react";

import { Button, PriorityChip, ToolAvatar, Tooltip, useFloatingStyle } from "../components";
import { TOOLTIP_DELAY } from "../components/Tooltip";
import type { CardActionKind } from "../generated/CardActionKind";
import type { CardStatusHover } from "../generated/CardStatusHover";
import type { CardStatusLine } from "../generated/CardStatusLine";
import type { TicketCardView } from "../generated/TicketCardView";
import { Icon } from "../theme/Icon";
import { type CardState, edgeColor, paletteInk, RESTING, toneInk } from "./look";

export interface TicketCardProps {
  card: TicketCardView;
  state?: CardState;
  /** A click on the card (selects it). */
  onSelect?: () => void;
  /** The card's one action. */
  onAction?: (kind: CardActionKind) => void;
  /** The quick action "Open panel". */
  onOpenPanel?: () => void;
}

/** The key row's hover (BRD-03): the full status ("Needs you · permission"), then the facts the card has no room for
 * ("Tokens: 241k", "Branch otaq/SHOP-25"), one per line. */
export function StatusHoverCard({ hover }: { hover: CardStatusHover }) {
  return (
    <>
      <div className="card-hover-title">
        <strong>{hover.status}</strong>
      </div>
      {hover.details.map((line) => (
        <span key={line} className="card-hover-text" data-muted>
          {line}
        </span>
      ))}
    </>
  );
}

/** The floating card, placed next to its anchor once it is mounted. */
function HoverBubble({ anchor, children }: { anchor: RefObject<HTMLElement | null>; children: ReactNode }) {
  const card = useRef<HTMLDivElement>(null);
  const style = useFloatingStyle(anchor, card);
  return (
    <div ref={card} role="tooltip" className="card-hover" style={style}>
      {children}
    </div>
  );
}

/** Shows `content` after a short rest of the pointer on (or focus in) what it wraps; Esc, leave, blur, a pointer
 * down (the click that opens the sheet) or the card becoming selected hide it (#6). */
function WithHover({
  content,
  selected,
  children,
}: {
  content: ReactNode;
  selected: boolean;
  children: ReactNode;
}) {
  const anchor = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
    },
    [],
  );
  const show = (delay: number) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      setOpen(true);
    }, delay);
  };
  const hide = () => {
    window.clearTimeout(timer.current);
    setOpen(false);
  };
  // Selecting the card opens its sheet over the board (or moves the board's selection to it): the bubble goes, and
  // a show still waiting for its delay is dropped.
  const [wasSelected, setWasSelected] = useState(selected);
  if (selected !== wasSelected) {
    setWasSelected(selected);
    if (selected) setOpen(false);
  }
  useEffect(() => {
    if (selected) window.clearTimeout(timer.current);
  }, [selected]);
  return (
    <span
      ref={anchor}
      className="card-hover-anchor"
      role="presentation"
      onMouseEnter={() => {
        show(TOOLTIP_DELAY);
      }}
      onMouseLeave={hide}
      onPointerDown={hide}
      onFocus={() => {
        show(0);
      }}
      onBlur={hide}
      onKeyDown={(e) => {
        if (e.key === "Escape") hide();
      }}
    >
      {children}
      {open && <HoverBubble anchor={anchor}>{content}</HoverBubble>}
    </span>
  );
}

/** The one status line: a 7 px dot in the state's colour (a blocked ticket's link, DPV-01), then the words (in the
 * edge's colour while the card needs the user). */
function StatusLine({ line }: { line: CardStatusLine }) {
  return (
    <div className="card-status">
      {line.icon === null ? (
        <span className="card-status-dot" style={{ background: paletteInk(line.dot) }} />
      ) : (
        <span className="card-status-icon" style={{ color: paletteInk(line.dot) }}>
          <Icon name={line.icon} size={12} />
        </span>
      )}
      <span className="card-status-text" style={{ color: toneInk(line.tone).fg }}>
        {line.text}
      </span>
    </div>
  );
}

/** Who and when, then the one action (*24 · The two views*): the tool mark and the agent's name on the left, the time
 * and the button on the right. */
function WhoRow({ card, onAction }: Pick<TicketCardProps, "card" | "onAction">) {
  const { avatar, who, time, action } = card;
  if (avatar === null && who === null && time === null && action === null) return null;
  return (
    <div className="card-who">
      {avatar && <ToolAvatar avatar={avatar} />}
      <span className="card-who-name">{who ?? ""}</span>
      {time && (
        <span className="card-time" data-stale={time.stale || undefined}>
          {time.text}
        </span>
      )}
      {action && (
        // The card's own click must not fire: the button's wrapper stops it.
        // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- only stops the click from reaching the card; the button inside is the control
        <span
          className="card-action"
          onClick={(e) => {
            e.stopPropagation();
          }}
        >
          <Button
            label={action.label}
            variant={action.primary ? "primary" : "secondary"}
            compact
            onClick={() => onAction?.(action.kind)}
          />
        </span>
      )}
    </div>
  );
}

function QuickActions({ card, onOpenPanel }: { card: TicketCardView; onOpenPanel?: () => void }) {
  return (
    <div className="card-quick">
      <Tooltip text={card.open_terminal}>
        <span className="card-quick-icon">
          <Icon name="square-terminal" size={13} />
        </span>
      </Tooltip>
      <Tooltip text={card.open_panel} keys={card.open_panel_keys}>
        <button
          type="button"
          className="card-quick-button"
          aria-label={card.open_panel}
          onClick={(e) => {
            e.stopPropagation();
            onOpenPanel?.();
          }}
        >
          <Icon name="panel-right" size={13} />
        </button>
      </Tooltip>
    </div>
  );
}

function Body({
  card,
  onAction,
  selected,
}: Pick<TicketCardProps, "card" | "onAction"> & { selected: boolean }) {
  return (
    <>
      <WithHover content={<StatusHoverCard hover={card.hover} />} selected={selected}>
        <div className="card-head">
          <span className="card-key">{card.key_label}</span>
          {card.priority !== null && <PriorityChip priority={card.priority} />}
          <span className="card-spacer" />
          {card.epic !== null && (
            <span className="card-epic">
              <Icon name="layers" size={11} />
              <span className="card-epic-name">{card.epic}</span>
            </span>
          )}
        </div>
      </WithHover>
      {/* The shared tooltip goes on any press or key (T-0506), so the click or ↵ that opens the sheet hides it. */}
      <Tooltip text={card.title}>
        <div className="card-title">{card.title}</div>
      </Tooltip>
      {card.status && <StatusLine line={card.status} />}
      {card.blocks !== null && (
        <div className="card-blocks">
          <Icon name="link" size={11} />
          <span>{card.blocks.line}</span>
        </div>
      )}
      {card.origin !== null && <div className="card-origin">{card.origin}</div>}
      <WhoRow card={card} {...(onAction ? { onAction } : {})} />
    </>
  );
}

export function TicketCard({ card, state = RESTING, onSelect, onAction, onOpenPanel }: TicketCardProps) {
  const chain = state.chain === "none" ? undefined : state.chain;
  return (
    // The card is a pointer target; the board's keyboard moves the selection (T-0220), so it isn't a tab stop.
    // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- the board's own keys select and open cards (SDD §21.3); a card isn't a button
    <article
      className="ticket-card"
      aria-label={`${card.key_label} ${card.title}`}
      data-selected={state.selected || undefined}
      data-chain={chain}
      data-dragging={state.dragging || undefined}
      data-hovered={state.hovered || undefined}
      data-faded={card.faded === null ? undefined : true}
      data-fresh={card.fresh || undefined}
      data-edge={card.edge ?? undefined}
      style={
        {
          ...(card.edge === null ? {} : { "--card-outline": edgeColor(card.edge) }),
          ...(card.faded === null ? {} : { "--card-faded": card.faded }),
        } as CSSProperties
      }
      onClick={onSelect}
    >
      <Body card={card} selected={state.selected} {...(onAction ? { onAction } : {})} />
      {(onOpenPanel !== undefined || state.hovered) && (
        <QuickActions card={card} {...(onOpenPanel ? { onOpenPanel } : {})} />
      )}
    </article>
  );
}
