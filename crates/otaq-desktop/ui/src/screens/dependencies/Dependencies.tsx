// The sheet's DEPENDENCIES (DPV-04, d0006, d0049; *24 → Adding and removing* `s24-11`), in Overview under the
// description: BLOCKED BY ("starts when both merge" while it waits) and BLOCKS, each row with the ticket's key, title
// and where it is now ("Review · your check"); a row opens that ticket, its ✕ removes the link. Add opens a picker:
// the direction first (Blocked by · Blocks), then a search by key or title; ↑↓ and ↵ pick, Esc closes. The service
// checks every link (loops, bases, who may) and a loop is refused with DPV-04's words, shown under the rows where the
// rule reads. Everything it says is Rust's (`dependencies.view`, `dependencies.search`, `dependencies.link`).
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from "react";

import type { TicketProps } from "../../app/registry";
import { openTicket } from "../../app/selection";
import { Popover, PLAIN_TEXT } from "../../components";
import { paletteInk } from "../../cards/look";
import type { DependenciesView } from "../../generated/DependenciesView";
import type { DependencyDirection } from "../../generated/DependencyDirection";
import type { DependencyGroup } from "../../generated/DependencyGroup";
import type { DependencyLinked } from "../../generated/DependencyLinked";
import type { DependencyRow } from "../../generated/DependencyRow";
import type { DependencySearchRow } from "../../generated/DependencySearchRow";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { DEPENDENCY_EVENTS, highlighted, step } from "./logic";

/** Sends one link's change; answers with the refusal's words, or `null` when it went through. */
export function sendLink(
  key: string,
  other: string,
  direction: DependencyDirection,
  add: boolean,
  ask: typeof request = request,
): Promise<string | null> {
  return (ask("dependencies.link", { key, other, direction, add }) as Promise<DependencyLinked>).then(
    (out) => (out.outcome === "refused" ? out.message : null),
  );
}

function Row({
  row,
  remove,
  onRemove,
}: {
  row: DependencyRow;
  remove: string;
  onRemove: (row: DependencyRow) => void;
}) {
  return (
    <div className="dependency-row">
      <button
        type="button"
        className="dependency-open"
        onClick={() => {
          openTicket(row.key);
        }}
      >
        <span className="dependency-key">{row.key}</span>
        <span className="dependency-title">{row.title}</span>
        <span className="dependency-place">
          <span className="dependency-dot" style={{ background: paletteInk(row.dot) }} />
          <span>{row.place}</span>
        </span>
      </button>
      <button
        type="button"
        className="dependency-remove"
        aria-label={`${remove} ${row.key}`}
        title={remove}
        onClick={() => {
          onRemove(row);
        }}
      >
        <Icon name="x" size={13} />
      </button>
    </div>
  );
}

function Group({
  group,
  remove,
  onRemove,
}: {
  group: DependencyGroup;
  remove: string;
  onRemove: (row: DependencyRow) => void;
}) {
  const label = group.label.toUpperCase();
  return (
    <div className="dependency-group" role="group" aria-label={group.label}>
      <div className="dependency-group-label">{group.note === null ? label : `${label} · ${group.note}`}</div>
      {group.rows.length === 0 ? (
        <div className="dependency-empty">{group.empty}</div>
      ) : (
        group.rows.map((row) => <Row key={row.key} row={row} remove={remove} onRemove={onRemove} />)
      )}
    </div>
  );
}

export interface PickerProps {
  view: DependenciesView;
  direction: DependencyDirection;
  onDirection: (direction: DependencyDirection) => void;
  query: string;
  onQuery: (query: string) => void;
  rows: readonly DependencySearchRow[];
  refusal: string | null;
  onPick: (row: DependencySearchRow) => void;
}

/** The Add picker's body: the direction, the search and its rows, then the loop rule (or the refusal). */
export function PickerBody({
  view,
  direction,
  onDirection,
  query,
  onQuery,
  rows,
  refusal,
  onPick,
}: PickerProps) {
  const [hover, setHover] = useState<number | null>(null);
  const lit = highlighted(hover, rows.length);
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      setHover((h) => step(h, e.key === "ArrowDown" ? 1 : -1, rows.length));
    } else if (e.key === "Enter") {
      const row = lit === null ? undefined : rows[lit];
      if (row) onPick(row);
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div className="dependency-picker">
      <div className="board-segments" role="radiogroup" aria-label={view.add}>
        {view.directions.map((d) => (
          <button
            key={d.direction}
            type="button"
            role="radio"
            className="board-segment"
            aria-checked={d.direction === direction}
            tabIndex={d.direction === direction ? 0 : -1}
            onClick={() => {
              onDirection(d.direction);
            }}
          >
            {d.label}
          </button>
        ))}
      </div>
      <label className="dependency-search">
        <Icon name="search" size={13} />
        <input
          {...PLAIN_TEXT}
          // The picker opens to type into.
          // eslint-disable-next-line jsx-a11y/no-autofocus -- the Add click asks for the search
          autoFocus
          value={query}
          placeholder={view.search}
          aria-label={view.search}
          onChange={(e) => {
            onQuery(e.target.value);
            setHover(null);
          }}
          onKeyDown={onKey}
        />
      </label>
      {rows.length > 0 && (
        <div className="dependency-results" role="listbox" aria-label={view.search}>
          {rows.map((row, i) => (
            <div
              key={row.key}
              role="option"
              tabIndex={-1}
              aria-selected={i === lit}
              className="dependency-result"
              data-lit={i === lit || undefined}
              onMouseEnter={() => {
                setHover(i);
              }}
              // Picking must not blur the search first.
              onMouseDown={(e) => {
                e.preventDefault();
                onPick(row);
              }}
            >
              <span className="dependency-key">{row.key}</span>
              <span className="dependency-result-title">{row.title}</span>
              <span className="dependency-result-place">{row.place}</span>
            </div>
          ))}
        </div>
      )}
      {refusal === null ? (
        <p className="dependency-rule">{view.rule}</p>
      ) : (
        <p className="dependency-rule" data-refused role="alert">
          {refusal}
        </p>
      )}
    </div>
  );
}

/** The section for `view`; the gallery draws it too. */
export function DependenciesBody({ ticket, view }: { ticket: string; view: DependenciesView }) {
  const add = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [direction, setDirection] = useState<DependencyDirection>("blocked_by");
  const [query, setQuery] = useState("");
  const [rows, setRows] = useState<readonly DependencySearchRow[]>([]);
  const [refusal, setRefusal] = useState<string | null>(null);
  useEffect(() => {
    if (!open) return;
    let live = true;
    (
      request("dependencies.search", { key: ticket, direction, query }) as Promise<DependencySearchRow[]>
    ).then(
      (r) => {
        if (live) setRows(r);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [open, ticket, direction, query]);
  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setRefusal(null);
  }, []);
  const pick = (row: DependencySearchRow) => {
    setRefusal(null);
    void sendLink(ticket, row.key, direction, true).then((refused) => {
      if (refused === null) close();
      else setRefusal(refused);
    });
  };
  const [removeError, setRemoveError] = useState<string | null>(null);
  const remove = (row: DependencyRow) => {
    setRemoveError(null);
    void sendLink(ticket, row.key, row.direction, false).then(setRemoveError);
  };
  return (
    <section className="dependencies" aria-label={view.title}>
      <div className="overview-label">
        <span className="editor-section-name">{view.title.toUpperCase()}</span>
        <span className="editor-spacer" />
        <button
          ref={add}
          type="button"
          className="overview-add"
          aria-expanded={open}
          onClick={() => {
            if (open) close();
            else setOpen(true);
          }}
        >
          <Icon name="plus" size={12} />
          {view.add}
        </button>
      </div>
      <Group group={view.blocked_by} remove={view.remove} onRemove={remove} />
      <Group group={view.blocks} remove={view.remove} onRemove={remove} />
      {removeError !== null && (
        <p className="dependency-rule" data-refused role="alert">
          {removeError}
        </p>
      )}
      {open && (
        <Popover label={view.add} anchor={add} align="end" onClose={close}>
          <PickerBody
            view={view}
            direction={direction}
            onDirection={(d) => {
              setDirection(d);
              setRefusal(null);
            }}
            query={query}
            onQuery={(q) => {
              setQuery(q);
              setRefusal(null);
            }}
            rows={rows}
            refusal={refusal}
            onPick={pick}
          />
        </Popover>
      )}
    </section>
  );
}

export function DependenciesSection({ ticket }: TicketProps) {
  const load = useCallback(
    () => request("dependencies.view", { key: ticket }) as Promise<DependenciesView | null>,
    [ticket],
  );
  const { view } = useServiceView(load, DEPENDENCY_EVENTS);
  if (view?.key !== ticket) return null;
  // A new ticket starts with the picker closed.
  return <DependenciesBody key={ticket} ticket={ticket} view={view} />;
}
