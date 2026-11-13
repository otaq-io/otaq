// Placing a floating layer (menu, popover, tooltip) next to its anchor: fixed to the window, so a scrolling or
// clipping parent never cuts it, and drawn in the overlay host (`FloatingPortal`), above every panel, so no panel's
// stacking context can put it under another panel (OTAQ-3: the ticket sheet, z-index 8, drew its pickers under the
// expanded Noyan panel). A layer opened inside another layer (a menu in a popover, a picker in a dialog) stays inside
// that layer's element instead, so a press in it is inside its owner, Esc closes it first and the dialog's keys skip
// it. The theme is set on the root element, so a portaled layer keeps it.
import {
  createContext,
  type CSSProperties,
  type ReactNode,
  type RefObject,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

import { placeFloating, type Rect } from "./logic";
import { useEnterExit } from "./motion";

/** The element the frame's overlay host draws into (`app/Frame.tsx`), above every panel. Absent outside the frame (a
 * test, the gallery). */
export const OVERLAY_HOST_ID = "overlay-host";

function overlayHost(): HTMLElement | null {
  return typeof document === "undefined" ? null : document.getElementById(OVERLAY_HOST_ID);
}

/** Whether what renders here is already inside a layer drawn in the overlay host (a popover, a dialog, Settings). */
const InLayer = createContext(false);

/** Marks `children` as inside a layer: for what the overlay host draws directly (the frame's overlays). */
export function InsideLayer({ children }: { children: ReactNode }) {
  return <InLayer.Provider value={true}>{children}</InLayer.Provider>;
}

/**
 * `child` in the overlay host (or in `slot`, an element inside it: the full-window view's box), marked as inside a
 * layer, so the floating layers opened in it stay in it; in place where there is no host (or no page at all: a static
 * render). Everything drawn into the overlay host goes through this: dialogs, the frame's overlays, the full view.
 */
export function inOverlayHost(child: ReactNode, slot?: HTMLElement): ReactNode {
  const target = slot ?? overlayHost();
  const marked = <InsideLayer>{child}</InsideLayer>;
  return target === null ? marked : createPortal(marked, target);
}

/**
 * The root of every floating layer: drawn in the overlay host, above every panel, unless it is already inside a layer
 * (then in place, inside that layer's element: `usePressOutside`'s containment, Esc's order and a dialog's keys rely on
 * it), or there is no host (a test without the frame, the gallery: in place, as before).
 */
export function FloatingPortal({ children }: { children: ReactNode }) {
  const inLayer = useContext(InLayer);
  const host = inLayer ? null : overlayHost();
  return host === null ? children : createPortal(<InsideLayer>{children}</InsideLayer>, host);
}

/** What a floating layer sits next to: an element, or a point's rectangle (a context menu at the pointer). */
export type Anchor = RefObject<HTMLElement | null> | Rect;

function isRef(anchor: Anchor): anchor is RefObject<HTMLElement | null> {
  return "current" in anchor;
}

/**
 * Calls `onPress` on a press outside `inside` and outside `anchor` (whose own click toggles the layer); nothing while
 * `onPress` is undefined. Heard while capturing, so a handler that stops the press can't keep a layer open, and on the
 * window's drag region too, which moves no focus. Every open list, menu or popover closes through this (most through
 * `useFloatingStyle`'s `onDismiss`): one that waits for its blur stays open whenever it never had the keyboard.
 */
export function usePressOutside(
  inside: RefObject<HTMLElement | null>,
  anchor: RefObject<HTMLElement | null> | null | undefined,
  onPress: (() => void) | undefined,
): void {
  const latest = useRef(onPress);
  useEffect(() => {
    latest.current = onPress;
  });
  const listening = onPress !== undefined;
  useEffect(() => {
    if (!listening) return;
    const onDown = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (inside.current?.contains(target) || anchor?.current?.contains(target)) return;
      latest.current?.();
    };
    document.addEventListener("pointerdown", onDown, true);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
    };
  }, [listening, inside, anchor]);
}

/**
 * The fixed position of `floating` next to `anchor` (see `placeFloating`), measured before the first paint. Hidden
 * until measured, so it never flashes at the window's corner.
 *
 * A layer that can close (a menu, a dropdown, a popover) passes `onDismiss`, and this closes it the way a native popup
 * closes: on a press outside it and outside its anchor element (whose own click toggles it), and on a scroll anywhere
 * behind it, which moves its anchor (`usePressOutside`). Every such layer gets both here, so none has to remember
 * them. A layer that can't close (a tooltip) follows its anchor instead. A scroll or press inside the layer itself (a
 * long list) is its own and changes nothing.
 *
 * It fades in, dropping 4 px into place, and back out the same way when it closes (OT-6).
 */
export function useFloatingStyle(
  anchor: Anchor,
  floating: RefObject<HTMLElement | null>,
  align: "start" | "end" = "start",
  onDismiss?: () => void,
): CSSProperties {
  useEnterExit(floating, "floating");
  const [position, setPosition] = useState<{ left: number; top: number }>();
  // A rectangle is a new object on each render: compare it by value.
  const ref = isRef(anchor) ? anchor : null;
  const rect = isRef(anchor) ? null : anchor;
  const rectKey = rect
    ? `${String(rect.left)},${String(rect.top)},${String(rect.width)},${String(rect.height)}`
    : "";
  // The latest callback, so an owner's inline function doesn't re-place the layer on every render.
  const dismiss = useRef(onDismiss);
  useEffect(() => {
    dismiss.current = onDismiss;
  });
  usePressOutside(floating, ref, onDismiss);
  useLayoutEffect(() => {
    const f = floating.current;
    if (!f) return;
    const place = () => {
      const box = ref ? ref.current?.getBoundingClientRect() : rect;
      if (!box) return;
      setPosition(
        placeFloating(
          box,
          { width: f.offsetWidth, height: f.offsetHeight },
          { width: window.innerWidth, height: window.innerHeight },
          align,
        ),
      );
    };
    const onScroll = (e: Event) => {
      if (e.target instanceof Node && f.contains(e.target)) return;
      if (dismiss.current) dismiss.current();
      else place();
    };
    place();
    window.addEventListener("resize", place);
    // Capturing: a scroll doesn't bubble, so this is how a scroll in any scrolling parent reaches the document.
    document.addEventListener("scroll", onScroll, true);
    // A layer that can't close (a hint, a tooltip) follows its element as it moves or appears without a scroll or a
    // resize of the window (the box for new work re-centres when the ticket sheet closes), and hides while the
    // element is gone or hidden.
    let frame = 0;
    let last: string | null = null;
    const follow = () => {
      const box = ref?.current?.getBoundingClientRect();
      const shown = box !== undefined && (box.width > 0 || box.height > 0);
      const key = shown
        ? `${String(box.left)},${String(box.top)},${String(box.width)},${String(box.height)}`
        : "";
      if (key !== last) {
        last = key;
        if (shown) place();
        else setPosition(undefined);
      }
      frame = requestAnimationFrame(follow);
    };
    if (ref && !dismiss.current) frame = requestAnimationFrame(follow);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      document.removeEventListener("scroll", onScroll, true);
    };
    // `rect` is compared by `rectKey`.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a rectangle anchor is a new object on every render; its value is `rectKey`
  }, [ref, rectKey, floating, align]);
  return position
    ? { position: "fixed", left: position.left, top: position.top }
    : { position: "fixed", left: 0, top: 0, visibility: "hidden" };
}
