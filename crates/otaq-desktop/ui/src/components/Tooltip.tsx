// Tooltip (*01 · Components → Overlays & feedback*): always dark, in both themes; the action's shortcut after the
// text, or a bold title with detail lines and a dimmer note (*02 → Header · running chip tooltip*, *Activity ·
// hover*). A short tooltip may lead with a small coloured icon (*05 → c · Copied state*: a green check before
// "Copied"). It shows after a short hover or on keyboard focus, and goes on leave, blur, a press or any key (Esc
// among them, T-0506).
//
// The webview doesn't always say the pointer left (T-0506, "Remove variable" and "Files ⌘⌥O · g f" stayed on screen):
// not when what was under it is removed or the view changes under a still pointer, nor when the pointer leaves the
// window fast. So while a tooltip shows, the window's own pointer moves, presses and keys hide it too: a move that is
// no longer over its anchor, the pointer leaving the window, the window losing focus.
import { type ReactNode, type RefObject, useCallback, useEffect, useId, useRef, useState } from "react";

import type { IconName } from "../generated/IconName";
import { Icon } from "../theme/Icon";
import { useFloatingStyle } from "./Floating";

/** How long the pointer rests before a tooltip shows, in ms. */
export const TOOLTIP_DELAY = 400;

export interface TooltipProps {
  text: string;
  /** The shortcut, as a Kbd-like hint after the text. */
  keys?: string | undefined;
  /** An icon before the text, in the tooltip's success green (a check before "Copied"). */
  icon?: IconName | undefined;
  /** Lines under a bold `text`; the last may be a dimmer `note`. */
  detail?: readonly string[];
  note?: string;
  children: ReactNode;
}

function Bubble({
  id,
  anchor,
  text,
  keys,
  icon,
  detail,
  note,
}: Omit<TooltipProps, "children"> & {
  id: string;
  anchor: RefObject<HTMLElement | null>;
}) {
  const bubble = useRef<HTMLDivElement>(null);
  const style = useFloatingStyle(anchor, bubble);
  const rich = detail !== undefined || note !== undefined;
  return (
    <div ref={bubble} id={id} role="tooltip" className="tooltip" data-rich={rich || undefined} style={style}>
      {rich ? (
        <>
          <strong>{text}</strong>
          {detail?.map((line) => (
            <span key={line}>{line}</span>
          ))}
          {note !== undefined && <span className="tooltip-note">{note}</span>}
        </>
      ) : (
        <>
          {icon !== undefined && <Icon name={icon} size={12} />}
          <span>{text}</span>
          {keys !== undefined && <kbd className="tooltip-keys">{keys}</kbd>}
        </>
      )}
    </div>
  );
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "OS", "AltGraph", "CapsLock", "Fn"]);

/** Wraps the element it explains. */
export function Tooltip({ children, ...bubble }: TooltipProps) {
  const id = useId();
  const anchor = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  // Shown by the pointer (hidden when it moves off the anchor) or by keyboard focus (kept while the focus stays).
  const by = useRef<"pointer" | "focus">("pointer");
  const show = (delay: number, how: "pointer" | "focus") => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      by.current = how;
      setOpen(true);
    }, delay);
  };
  // A press on the anchor focuses it: that focus isn't the keyboard's and shows nothing (a pressed Files button
  // kept its tooltip under the toolbar while the Files view opened).
  const pressed = useRef(false);
  const hide = useCallback(() => {
    window.clearTimeout(timer.current);
    setOpen(false);
  }, []);
  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
    },
    [],
  );
  // While it shows, the window's own events hide it too (see the top): heard while capturing, so a handler that stops
  // a key (the Esc of a field) or a press can't keep it on screen.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (!MODIFIER_KEYS.has(e.key)) hide();
    };
    const onMove = (e: PointerEvent) => {
      if (by.current !== "pointer") return;
      if (!(e.target instanceof Node) || !anchor.current?.contains(e.target)) hide();
    };
    const onOut = (e: PointerEvent) => {
      // Out of the window: nothing is under the pointer any more.
      if (e.relatedTarget === null && by.current === "pointer") hide();
    };
    const options = { capture: true } as const;
    window.addEventListener("keydown", onKey, options);
    window.addEventListener("pointerdown", hide, options);
    window.addEventListener("pointermove", onMove, options);
    document.addEventListener("pointerout", onOut, options);
    window.addEventListener("blur", hide);
    return () => {
      window.removeEventListener("keydown", onKey, options);
      window.removeEventListener("pointerdown", hide, options);
      window.removeEventListener("pointermove", onMove, options);
      document.removeEventListener("pointerout", onOut, options);
      window.removeEventListener("blur", hide);
    };
  }, [open, hide]);
  return (
    <span
      ref={anchor}
      className="tooltip-anchor"
      // Only listens for hover and focus of what it wraps; the wrapped element keeps its own role.
      role="presentation"
      onMouseEnter={() => {
        show(TOOLTIP_DELAY, "pointer");
      }}
      onMouseLeave={() => {
        pressed.current = false;
        hide();
      }}
      onPointerDownCapture={() => {
        pressed.current = true;
        hide();
      }}
      onPointerUp={() => {
        pressed.current = false;
      }}
      onFocus={() => {
        if (pressed.current) pressed.current = false;
        else show(0, "focus");
      }}
      onBlur={hide}
    >
      {children}
      {open && <Bubble id={id} anchor={anchor} {...bubble} />}
    </span>
  );
}
