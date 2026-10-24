// The epic's sheet (SHT-07; d0059, d0080, d0083; *25 → An epic opens like a ticket* `s25-11`): what an epic's sheet
// shows besides a ticket's. Its header's "1 of 5 merged" and the lead's state (`EpicHeadPills`); at the top of
// Overview, what the lead asks you with its options as buttons (`LeadAsks`; a click sends that option, Other… opens a
// field); under the description, TICKETS as a stage grid (`EpicGrid`: each child where it is now and what it is doing,
// a row opens it); the Tickets tab (`TicketsTab`: every child as a List row, then Add ticket to epic). Everything it
// says is Rust's (`panel.view`'s `sheet.epic` and `answer`, `epic.overview`, `epic.tickets`).
import { type KeyboardEvent, type PointerEvent, useCallback, useState } from "react";

import { requestCardAction } from "../../app/cardActions";
import { openOverlay } from "../../app/overlays";
import type { TicketProps } from "../../app/registry";
import { openTicket } from "../../app/selection";
import { paletteInk } from "../../cards/look";
import { Button, PLAIN_TEXT } from "../../components";
import type { EpicGrid as Grid } from "../../generated/EpicGrid";
import type { EpicHead } from "../../generated/EpicHead";
import type { EpicOverviewView } from "../../generated/EpicOverviewView";
import type { EpicTicketsView } from "../../generated/EpicTicketsView";
import type { PanelAnswer } from "../../generated/PanelAnswer";
import type { PanelSent } from "../../generated/PanelSent";
import type { PanelView } from "../../generated/PanelView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { dropCard } from "../board/Board";
import { Row, type ListHandlers } from "../list/List";
import { askUnsent } from "../sheet/actions";
import { canSend } from "../sheet/actionRoutes";
import { PANEL_EVENTS } from "../sheet/actions";
import "./epic.css";

const ignore = () => undefined;

/** "1 of 5 merged" and "Lead asks you" beside the epic's title (`s25-11`). */
export function EpicHeadPills({ head }: { head: EpicHead }) {
  return (
    <>
      <span className="epic-pill epic-pill-progress">
        <Icon name="git-merge" size={12} />
        <span>{head.progress}</span>
      </span>
      {head.lead !== null && (
        <span className="epic-pill epic-pill-lead" data-tone={head.lead.tone}>
          <Icon name="bot" size={12} />
          <span>{head.lead.label}</span>
        </span>
      )}
    </>
  );
}

/**
 * What the lead asks you (`s25-11`): "Wishlist lead asks", its question and its options as buttons, the recommended
 * one (else the first) filled. A click sends that option (`panel.answer`, typed into the lead's session; unsent text
 * there asks first); Other… opens a field for your own answer.
 */
export function LeadAsks({ view, answer }: { view: PanelView; answer: PanelAnswer }) {
  const [other, setOther] = useState<number | null>(null);
  const [text, setText] = useState("");
  const [sending, setSending] = useState<number | null>(null);
  const send = (pick: number, typed: string, clear = false): Promise<unknown> => {
    if (sending !== null || !canSend(pick, typed, answer.rows)) return Promise.resolve();
    setSending(pick);
    const params = { key: view.key, question: answer.question, pick, other: typed, clear };
    return (request("panel.answer", params) as Promise<PanelSent>)
      .then((sent) => {
        askUnsent(sent, () => send(pick, typed, true));
      })
      .catch(ignore)
      .finally(() => {
        setSending(null);
      });
  };
  const filled = answer.rows.find((r) => r.recommended) ?? answer.rows[0];
  return (
    <section className="lead-asks" aria-label={answer.asker ?? answer.text}>
      <Icon name="message-circle-question-mark" size={16} />
      <div className="lead-asks-text">
        {answer.asker !== null && <span className="lead-asks-heading">{answer.asker}</span>}
        <span className="lead-asks-question">{answer.text}</span>
        {other !== null && (
          <span className="lead-asks-other">
            <input
              {...PLAIN_TEXT}
              className="field-input"
              value={text}
              placeholder={answer.other_placeholder}
              aria-label={answer.other_placeholder}
              // The field appears because Other… was just clicked: typing goes straight into it.
              // eslint-disable-next-line jsx-a11y/no-autofocus -- it opens on the user's own click
              autoFocus
              onChange={(e) => {
                setText(e.target.value);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void send(other, text);
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                  setOther(null);
                }
              }}
            />
            <Button
              compact
              icon="send"
              label={answer.send}
              disabled={!canSend(other, text, answer.rows)}
              loading={sending === other}
              onClick={() => {
                void send(other, text);
              }}
            />
          </span>
        )}
      </div>
      <div className="lead-asks-options">
        {answer.rows.map((row) => (
          <Button
            key={row.number}
            compact
            label={row.label}
            variant={row === filled ? "primary" : "secondary"}
            loading={sending === row.number}
            onClick={() => {
              if (row.other) setOther(row.number);
              else void send(row.number, "");
            }}
          />
        ))}
      </div>
    </section>
  );
}

/** TICKETS as a stage grid (`s25-11`): Ready and each stage of the flow, a dot per child where it is now. */
export function EpicGrid({ grid }: { grid: Grid }) {
  return (
    <section className="epic-grid" aria-label={grid.heading}>
      <div className="editor-section-name epic-grid-heading">{grid.heading}</div>
      <div className="epic-grid-row epic-grid-head" aria-hidden>
        <span className="epic-grid-ticket">{grid.corner}</span>
        {grid.columns.map((c, i) => (
          <span key={`${String(i)}:${c}`} className="epic-grid-column">
            {c}
          </span>
        ))}
      </div>
      {grid.rows.map((row) => (
        <button
          key={row.key}
          type="button"
          className="epic-grid-row epic-grid-child"
          onClick={() => {
            openTicket(row.key);
          }}
        >
          <span className="epic-grid-ticket">
            <span className="epic-grid-name">
              <span className="epic-grid-key">{row.short}</span>
              <span className="epic-grid-title">{row.title}</span>
            </span>
            <span className="epic-grid-line" data-attention={row.attention || undefined}>
              {row.line}
            </span>
          </span>
          {row.cells.map((cell, i) => (
            <span key={i} className="epic-grid-column">
              <span
                className="epic-grid-dot"
                data-current={cell.current || undefined}
                data-ahead={cell.fill === null || undefined}
                style={cell.fill === null ? undefined : { background: paletteInk(cell.fill) }}
                aria-label={grid.columns[i]}
              />
            </span>
          ))}
        </button>
      ))}
    </section>
  );
}

/** Overview's part for an epic: the grid, loaded with the epic. */
export function useEpicOverview(ticket: string, epic: boolean): EpicOverviewView | null {
  const load = useCallback(
    () =>
      epic ? (request("epic.overview", { key: ticket }) as Promise<EpicOverviewView>) : Promise.resolve(null),
    [ticket, epic],
  );
  const { view } = useServiceView(load, PANEL_EVENTS);
  return view?.key === ticket ? view : null;
}

/** What a child's row does in the Tickets tab: a click or ↵ opens it, its action opens it and asks for it, Start
 * queues it as on the Board. Rows aren't dragged here. */
const HANDLERS: ListHandlers = {
  press: (key: string, e: PointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    openTicket(key);
  },
  key: (key: string, _target, e: KeyboardEvent) => {
    if (e.target === e.currentTarget && e.key === "Enter") {
      e.preventDefault();
      openTicket(key);
    }
  },
  action: (key, kind) => {
    openTicket(key);
    requestCardAction(key, kind);
  },
  start: (key, target) => {
    void dropCard(key, target, () => {
      openOverlay("local_merge", { ticket: key, menu: true });
    });
  },
  menu: ignore,
  open: (key) => {
    openTicket(key);
  },
};

/** The Tickets tab for a loaded view (also what the markup tests and the Gallery draw). */
export function TicketsBody({ view }: { view: EpicTicketsView }) {
  return (
    <div className="epic-tickets">
      <div className="list" role="region" aria-label={view.key}>
        <div className="list-header" aria-hidden>
          <span className="list-header-ticket">{view.header.ticket}</span>
          <span>{view.header.flow}</span>
          <span>{view.header.happening}</span>
          <span>{view.header.who}</span>
        </div>
        <div className="list-body">
          {view.empty !== null && <div className="list-group-empty">{view.empty}</div>}
          {view.rows.map((row, i) => (
            <Row
              key={row.card.key}
              row={row}
              target={null}
              selected={false}
              lifted={false}
              focusable={i === 0}
              handlers={HANDLERS}
            />
          ))}
        </div>
      </div>
      <div className="epic-tickets-add">
        <button
          type="button"
          className="section-link"
          onClick={() => {
            openOverlay("new_ticket", { epic: view.key });
          }}
        >
          <Icon name="plus" size={12} />
          {view.add}
        </button>
        {view.next_key !== null && <span className="epic-tickets-next">{view.next_key}</span>}
      </div>
      <p className="epic-tickets-note">{view.note}</p>
    </div>
  );
}

/** The epic's Tickets tab (`sheetTabs` "tickets"). */
export function TicketsTab({ ticket }: TicketProps) {
  const load = useCallback(
    () => request("epic.tickets", { key: ticket }) as Promise<EpicTicketsView>,
    [ticket],
  );
  const { view } = useServiceView(load, PANEL_EVENTS);
  return view?.key === ticket ? <TicketsBody view={view} /> : null;
}
