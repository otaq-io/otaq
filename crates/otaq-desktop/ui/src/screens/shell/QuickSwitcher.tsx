// The quick switcher, ⌘P (UI-01, d0058; *02 → Quick switcher · ⌘P*): type to filter by name or key, ↑/↓ to move, ↵
// to switch, Esc to close, ⌘N to add a project. Projects that need you come first, then working ones, then idle ones
// (`shell.quick`, from the rail's last list); the rail itself keeps the user's order.
import { useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { type OverlayProps, useShortcut } from "../../app/registry";
import { selectProject } from "../../app/selection";
import { Kbd, useDismiss, PLAIN_TEXT } from "../../components";
import type { QuickSwitcherParams } from "../../generated/QuickSwitcherParams";
import type { QuickSwitcherRow } from "../../generated/QuickSwitcherRow";
import type { QuickSwitcherView } from "../../generated/QuickSwitcherView";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { clampCursor, switcherKey } from "./logic";

export interface QuickListProps {
  view: QuickSwitcherView;
  cursor: number;
  onChoose: (row: QuickSwitcherRow) => void;
  onHover?: (index: number) => void;
}

/** The rows and the footer's hints. */
export function QuickList({ view, cursor, onChoose, onHover }: QuickListProps) {
  return (
    <>
      <div className="quick-list" role="listbox" aria-label="Projects">
        {view.rows.map((row, i) => (
          <div
            key={row.project.key}
            role="option"
            tabIndex={-1}
            className="quick-row"
            aria-selected={i === cursor}
            data-idle={row.project.idle || undefined}
            onClick={() => {
              onChoose(row);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") onChoose(row);
            }}
            onMouseMove={() => onHover?.(i)}
          >
            <span className="quick-tile">{row.project.key}</span>
            <span className="quick-text">
              <span className="quick-label">{row.label}</span>
              <span className="quick-path">{row.project.path}</span>
            </span>
            {row.running !== null && (
              <span className="quick-running">
                <Icon name="activity" size={11} />
                {row.running}
              </span>
            )}
            {row.project.needs_you > 0 && (
              <span className="quick-needs">
                <Icon name="hand" size={10} />
                {row.project.needs_you}
              </span>
            )}
            {row.project.idle && <span className="quick-idle">idle</span>}
            {row.project.shortcut !== null && <span className="quick-keys">{row.project.shortcut}</span>}
          </div>
        ))}
      </div>
      <div className="quick-footer">
        {view.hints.map((hint) => (
          <span key={hint.label} className="quick-hint">
            <Kbd keys={hint.keys} />
            {hint.label}
          </span>
        ))}
      </div>
    </>
  );
}

/** The `quick_switcher` overlay. */
export function QuickSwitcher({ close }: OverlayProps<"quick_switcher">) {
  const box = useRef<HTMLDivElement>(null);
  const [filter, setFilter] = useState("");
  const [view, setView] = useState<QuickSwitcherView>();
  const [cursor, setCursor] = useState(0);
  useDismiss(box, undefined, close);

  useEffect(() => {
    let current = true;
    const params: QuickSwitcherParams = { filter };
    request("shell.quick", params).then(
      (answer) => {
        if (current) setView(answer as QuickSwitcherView);
      },
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [filter]);

  const rows = view?.rows ?? [];
  const index = clampCursor(cursor, rows.length);
  const choose = (row: QuickSwitcherRow) => {
    selectProject(row.project.key);
    close();
  };
  // ⌘N adds a project while the switcher is open, instead of a new ticket.
  const addProject = useCallback(() => {
    close();
    openOverlay("add_project");
  }, [close]);
  useShortcut("new_ticket", addProject);

  return (
    <div ref={box} className="quick" role="dialog" aria-label="Switch project">
      <div className="quick-input">
        <Icon name="folder-git-2" size={15} />
        <input
          {...PLAIN_TEXT}
          // The switcher opens to type into.
          // eslint-disable-next-line jsx-a11y/no-autofocus -- ⌘P opens it to type a project's name at once
          autoFocus
          value={filter}
          placeholder={view?.placeholder}
          aria-label={view?.placeholder ?? "Switch project"}
          onChange={(e) => {
            setFilter(e.target.value);
            setCursor(0);
          }}
          onKeyDown={(e) => {
            const step = switcherKey(e.key, index, rows.length);
            switch (step.kind) {
              case "move":
                setCursor(step.index);
                break;
              case "choose": {
                const row = rows[step.index];
                if (row) choose(row);
                break;
              }
              case "close":
                close();
                break;
              case "pass":
                return;
            }
            e.preventDefault();
          }}
        />
        <Kbd keys="Esc" />
      </div>
      {view && <QuickList view={view} cursor={index} onChoose={choose} onHover={setCursor} />}
    </div>
  );
}
