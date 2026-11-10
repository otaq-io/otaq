// Popover (*01 · Components → Overlays & feedback*; e.g. *02 → Header · agent limits ON + inline popover*): a raised
// panel next to its trigger, kept inside the window. Esc or a press outside it (and outside its trigger) closes it.
import { type ReactNode, type RefObject, useRef } from "react";

import { useEscapeLayer } from "./escape";
import { type Anchor, useFloatingStyle, usePressOutside } from "./Floating";

export interface PopoverProps {
  /** What the popover is, for screen readers. */
  label: string;
  anchor: Anchor;
  align?: "start" | "end";
  onClose: () => void;
  children: ReactNode;
}

/** Calls `onClose` on Esc wherever the focus is, while it is the topmost open layer (`escape.ts`): before any other
 * key handler of the page (the panel's Close). `element` is the layer's own, so one opened inside it is above it. */
export function useEscape(onClose: () => void, element?: RefObject<HTMLElement | null>): void {
  useEscapeLayer(onClose, element);
}

/**
 * Calls `onClose` on Esc and on a press outside `inside` (and outside `anchor`, whose own click toggles): for a box
 * that isn't a floating layer (the quick switcher). A floating layer gets the press from `useFloatingStyle`'s
 * `onDismiss` and needs only `useEscape`.
 */
export function useDismiss(
  inside: RefObject<HTMLElement | null>,
  anchor: Anchor | undefined,
  onClose: () => void,
): void {
  useEscape(onClose, inside);
  usePressOutside(inside, anchor && "current" in anchor ? anchor : null, onClose);
}

export function Popover({ label, anchor, align, onClose, children }: PopoverProps) {
  const floating = useRef<HTMLDivElement>(null);
  const style = useFloatingStyle(anchor, floating, align, onClose);
  useEscape(onClose, floating);
  return (
    <div ref={floating} className="popover" role="dialog" aria-label={label} style={style}>
      {children}
    </div>
  );
}
