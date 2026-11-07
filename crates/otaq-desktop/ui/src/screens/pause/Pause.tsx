// Pause in the title bar (T-0457; PAU-08; d0112, d0097): after Running, "Pause" with its icon, shown only while
// agents run (a paused project's Resume is its banner's). A click or ⇧⌘. opens the scope menu: "Pause shop-web" first
// and highlighted (↵ picks it), "Pause all projects", each saying what it stops, and the line under them. A pick
// pauses at once: no confirmation, since nothing is lost. Every word and count is `pause.view`'s.
import { type RefObject, useCallback, useEffect, useRef, useState } from "react";

import { useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { Kbd, Popover, Tooltip } from "../../components";
import type { PauseButton as PauseButtonView } from "../../generated/PauseButton";
import type { PauseMenu } from "../../generated/PauseMenu";
import type { PauseScopeRow } from "../../generated/PauseScopeRow";
import type { PauseView } from "../../generated/PauseView";
import type { PauseViewParams } from "../../generated/PauseViewParams";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { closePauseMenu, menuKey, PAUSE_EVENTS, pauseMenu, runParams, togglePauseMenu } from "./logic";

export interface PauseButtonProps {
  button: PauseButtonView;
  /** The menu is open: the outline turns to the focus colour. */
  open?: boolean;
  anchor?: RefObject<HTMLButtonElement | null>;
  onClick?: () => void;
}

/** Pause, its icon and its label. */
export function PauseButton({ button, open = false, anchor, onClick }: PauseButtonProps) {
  return (
    <Tooltip text={button.tooltip} keys={button.keys ?? undefined}>
      <button
        ref={anchor}
        type="button"
        className="header-pause"
        data-open={open || undefined}
        aria-label={button.label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={onClick}
      >
        <Icon name={button.icon} size={13} />
        <span>{button.label}</span>
      </button>
    </Tooltip>
  );
}

export interface PauseMenuBoxProps {
  menu: PauseMenu;
  cursor: number;
  onPick?: (row: PauseScopeRow) => void;
  onHover?: (index: number) => void;
}

/** The menu's content: the scopes and the line. */
export function PauseMenuBox({ menu, cursor, onPick, onHover }: PauseMenuBoxProps) {
  return (
    <div className="pause-menu" role="menu" aria-label={menu.title}>
      {menu.rows.map((row, i) => (
        <button
          key={row.scope}
          type="button"
          role="menuitem"
          className="pause-row"
          data-highlighted={i === cursor || undefined}
          onMouseMove={() => onHover?.(i)}
          onClick={() => onPick?.(row)}
        >
          <Icon name={row.icon} size={14} />
          <span className="pause-row-text">
            <span className="pause-row-label">{row.label}</span>
            <span className="pause-row-detail">{row.detail}</span>
          </span>
          {row.keys !== null && <Kbd keys={row.keys} />}
        </button>
      ))}
      {menu.note !== null && (
        <>
          <div className="pause-menu-rule" />
          <p className="pause-menu-note">{menu.note}</p>
        </>
      )}
    </div>
  );
}

/** The open menu under its button: ↑ ↓ ↵ and Esc, a press outside closes it. */
function MenuPopover({
  view,
  anchor,
  onDone,
}: {
  view: PauseView;
  anchor: RefObject<HTMLButtonElement | null>;
  onDone: (next: PauseView | null) => void;
}) {
  const [cursor, setCursor] = useState(0);
  const pick = useCallback(
    (row: PauseScopeRow) => {
      request("pause.run", runParams(row, view)).then(
        (next) => {
          onDone(next as PauseView);
        },
        () => {
          onDone(null);
        },
      );
    },
    [view, onDone],
  );
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const step = menuKey(e.key, cursor, view.menu.rows.length);
      if (step.kind === "pass") return;
      e.preventDefault();
      e.stopPropagation();
      if (step.kind === "move") setCursor(step.index);
      else {
        const row = view.menu.rows[step.index];
        if (row) pick(row);
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
    };
  }, [cursor, view, pick]);
  return (
    <Popover label={view.menu.title} anchor={anchor} align="end" onClose={closePauseMenu}>
      <PauseMenuBox menu={view.menu} cursor={cursor} onPick={pick} onHover={setCursor} />
    </Popover>
  );
}

/** The `header_pause` slot: the button while agents run, and its menu while open. */
export function HeaderPause() {
  const { project } = useSelection();
  const open = useStore(pauseMenu);
  const anchor = useRef<HTMLButtonElement>(null);
  const load = useCallback(() => {
    const params: PauseViewParams = project === null ? {} : { project };
    return request("pause.view", params) as Promise<PauseView>;
  }, [project]);
  const { view } = useServiceView(load, PAUSE_EVENTS);
  // The answer to a pick, shown until the service's events bring the next view.
  const [after, setAfter] = useState<{ before: PauseView; next: PauseView }>();
  const shown = after && after.before === view ? after.next : view;
  const done = useCallback(
    (next: PauseView | null) => {
      closePauseMenu();
      if (next && view) setAfter({ before: view, next });
    },
    [view],
  );
  useEffect(() => closePauseMenu, []);
  // Nothing runs: no Pause, and ⇧⌘. has no menu to open.
  const hidden = shown === undefined || !shown.shown;
  useEffect(() => {
    if (hidden) closePauseMenu();
  }, [hidden, open]);
  if (shown === undefined || hidden) return null;
  return (
    <>
      <PauseButton button={shown.button} open={open} anchor={anchor} onClick={togglePauseMenu} />
      {open && <MenuPopover view={shown} anchor={anchor} onDone={done} />}
    </>
  );
}
