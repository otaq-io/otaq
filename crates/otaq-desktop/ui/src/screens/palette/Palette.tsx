// The command palette, ⌘K / ⇧⌘P (KEY-06, d0097; *02 · Command palette*, s02-19 … s02-23): a non-modal overlay 96 px
// from the top over a dimmed window. Type to search tickets, projects and actions (`>` first: actions only); ↑ ↓ move
// over the rows that can run, ↵ runs or opens, Esc or a click outside closes. Esc and running an action give the
// keyboard back to where it was (a terminal keeps its focus). The rows and their words are Rust's (`palette.view`).
import { Fragment, type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { type OverlayProps, useRegistries } from "../../app/registry";
import { openTicket, selectProject, selection } from "../../app/selection";
import { Kbd, PLAIN_TEXT } from "../../components";
import type { PaletteParams } from "../../generated/PaletteParams";
import type { PaletteRow } from "../../generated/PaletteRow";
import type { PaletteView } from "../../generated/PaletteView";
import type { PauseRunParams } from "../../generated/PauseRunParams";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { ACTIONS_PREFIX, firstPickable, joinQuery, paletteKey, splitQuery } from "./logic";

/** An action's keys: "⌘N or c". */
function Keys({ keys, dim }: { keys: readonly string[]; dim: boolean }) {
  if (keys.length === 0) return null;
  return (
    <span className="palette-keys" data-dim={dim || undefined}>
      {keys.map((k, i) => (
        <Fragment key={k}>
          {i > 0 && <span className="palette-or">or</span>}
          <Kbd keys={k} />
        </Fragment>
      ))}
    </span>
  );
}

function RowContent({ row }: { row: PaletteRow }) {
  const item = row.item;
  switch (item.kind) {
    case "action":
      return (
        <>
          <Icon name={item.icon} size={14} />
          <span className="palette-title">{item.title}</span>
          {item.reason !== null && <span className="palette-aside">{item.reason}</span>}
          <span className="palette-fill" />
          <Keys keys={item.keys} dim={!row.pickable} />
        </>
      );
    case "ticket":
      return (
        <>
          <span className="palette-key">{item.key}</span>
          <span className="palette-title">{item.title}</span>
          {item.other_project !== null && <span className="palette-aside">{item.other_project}</span>}
          <span className="palette-fill" />
          <span className="palette-place">{item.place}</span>
        </>
      );
    case "project":
      return (
        <>
          <Icon name="folder-git-2" size={14} />
          <span className="palette-title">{item.title}</span>
          <span className="palette-fill" />
          <Keys keys={item.keys} dim={false} />
        </>
      );
    case "pause":
      return (
        <>
          <Icon name={item.icon} size={14} />
          <span className="palette-title">{item.title}</span>
        </>
      );
    case "divider":
      return null;
  }
}

export interface PaletteBoxProps {
  view: PaletteView;
  /** The field: the real input, or the gallery's still text. */
  field: ReactNode;
  cursor: number;
  onPick?: (row: PaletteRow) => void;
  onHover?: (index: number) => void;
}

/** The palette's box: the field's row, the list (or "Nothing matches") and the footer. */
export function PaletteBox({ view, field, cursor, onPick, onHover }: PaletteBoxProps) {
  const highlighted = view.rows[cursor];
  return (
    <>
      <div className="palette-field">
        <Icon name="search" size={16} />
        {field}
        {view.actions_only === null ? (
          <span className="palette-hint">
            <Kbd keys={view.hint_key} />
            {view.hint}
          </span>
        ) : (
          <span className="palette-hint" data-on>
            {view.actions_only}
          </span>
        )}
      </div>
      <div className="palette-list" role="listbox" aria-label={view.placeholder}>
        {view.section !== null && <div className="palette-section">{view.section}</div>}
        {view.rows.map((row, i) =>
          row.item.kind === "divider" ? (
            <div key={`divider-${String(i)}`} className="palette-divider" role="separator">
              <span />
              {row.item.label}
              <span />
            </div>
          ) : (
            <div
              key={`${row.item.kind}-${String(i)}`}
              role="option"
              tabIndex={-1}
              className="palette-row"
              aria-selected={i === cursor}
              aria-disabled={!row.pickable || undefined}
              onClick={() => {
                if (row.pickable) onPick?.(row);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && row.pickable) onPick?.(row);
              }}
              onMouseMove={() => {
                if (row.pickable) onHover?.(i);
              }}
            >
              <RowContent row={row} />
            </div>
          ),
        )}
        {view.nothing !== null && (
          <div className="palette-nothing">
            <span className="palette-nothing-title">
              <Icon name={view.nothing.icon} size={15} />
              {view.nothing.title}
            </span>
            <span className="palette-nothing-hint">{view.nothing.hint}</span>
          </div>
        )}
      </div>
      <div className="palette-footer">
        <Kbd keys={view.footer.move_keys[0]} />
        <Kbd keys={view.footer.move_keys[1]} />
        <span>{view.footer.move_label}</span>
        <span className="palette-gap" />
        <Kbd keys={view.footer.run_key} />
        <span>{highlighted?.item.kind === "ticket" ? view.footer.open : view.footer.run}</span>
        <span className="palette-gap" />
        <Kbd keys={view.footer.close_key} />
        <span>{view.footer.close}</span>
        <span className="palette-fill" />
        <span className="palette-opens">{view.footer.opens}</span>
      </div>
    </>
  );
}

/** The `command_palette` overlay. */
export function Palette({ params, close }: OverlayProps<"command_palette">) {
  const app = useRegistries();
  const box = useRef<HTMLDivElement>(null);
  // Where the keyboard was when it opened (the focus moves into the field at once).
  const [returnTo] = useState(() =>
    document.activeElement instanceof HTMLElement ? document.activeElement : null,
  );
  const terminal = params?.terminal ?? false;
  const [query, setQuery] = useState("");
  const [view, setView] = useState<PaletteView>();
  const [cursor, setCursor] = useState(-1);

  useEffect(() => {
    let current = true;
    const { project, ticket } = selection.get();
    const ask: PaletteParams = { query, project, ticket, terminal };
    request("palette.view", ask).then(
      (answer) => {
        if (!current) return;
        const next = answer as PaletteView;
        setView(next);
        setCursor(firstPickable(next.rows));
      },
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [query, terminal]);

  /** Closes, giving the keyboard back to where it was. */
  const done = useCallback(() => {
    if (returnTo?.isConnected) returnTo.focus();
    close();
  }, [returnTo, close]);

  // A press outside closes it without moving the focus back (the press puts it where it lands).
  useEffect(() => {
    const onPress = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node | null)) close();
    };
    document.addEventListener("pointerdown", onPress);
    return () => {
      document.removeEventListener("pointerdown", onPress);
    };
  }, [close]);

  const pick = (row: PaletteRow) => {
    const item = row.item;
    switch (item.kind) {
      case "action":
        done();
        app.shortcuts.run(item.action);
        return;
      case "ticket":
        close();
        if (selection.get().project !== item.project) selectProject(item.project);
        openTicket(item.key);
        return;
      case "project":
        close();
        selectProject(item.project);
        return;
      case "pause": {
        // At once, no confirmation (d0112); the title bar and the banner follow the service's events.
        close();
        const params: PauseRunParams = { scope: item.scope, resume: item.resume };
        if (item.project !== null) params.project = item.project;
        request("pause.run", params).catch(() => undefined);
        return;
      }
      case "divider":
        return;
    }
  };

  const rows = view?.rows ?? [];
  const { actionsOnly, value } = splitQuery(query);
  const field = (
    <>
      {actionsOnly && <span className="palette-prefix">{ACTIONS_PREFIX}</span>}
      <input
        {...PLAIN_TEXT}
        // The palette opens to type into.
        // eslint-disable-next-line jsx-a11y/no-autofocus -- ⌘K opens it to type at once
        autoFocus
        value={value}
        placeholder={actionsOnly ? undefined : view?.placeholder}
        aria-label={view?.placeholder ?? "Command palette"}
        onChange={(e) => {
          setQuery(joinQuery(actionsOnly, e.target.value));
        }}
        onKeyDown={(e) => {
          const step = paletteKey(e.key, cursor, rows, { value, actionsOnly });
          switch (step.kind) {
            case "move":
              setCursor(step.index);
              break;
            case "choose": {
              const row = rows[step.index];
              if (row) pick(row);
              break;
            }
            case "close":
              done();
              break;
            case "drop_prefix":
              setQuery("");
              break;
            case "pass":
              // ↵ on a greyed row does nothing, and nothing else gets it.
              if (e.key !== "Enter") return;
          }
          e.preventDefault();
          e.stopPropagation();
        }}
      />
    </>
  );

  return (
    <>
      <div className="palette-scrim" />
      <div ref={box} className="palette" role="dialog" aria-label="Command palette">
        {view ? (
          <PaletteBox view={view} field={field} cursor={cursor} onPick={pick} onHover={setCursor} />
        ) : (
          <div className="palette-field">
            <Icon name="search" size={16} />
            {field}
          </div>
        )}
      </div>
    </>
  );
}
