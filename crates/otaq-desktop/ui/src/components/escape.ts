// Esc closes the topmost open layer (a popover, a menu, a dialog) wherever the focus is, and only that one: never the
// ticket under it, nor a popover under a menu. One key handler for every layer, heard while capturing, so a focused
// element elsewhere (a terminal, a button behind the layer) can't keep the key. A layer that handles its own Esc while
// the focus is inside it (a menu, a dialog) is left to do so; one opened inside another layer is above it.
import { type RefObject, useEffect, useRef } from "react";

/** One open layer, as Esc sees it. */
export interface EscapeLayer {
  close: () => void;
  /** Its element, when it has one: a layer inside another's element is above it. */
  element: () => Element | null;
  /** It handles its own Esc while the focus is inside it. */
  ownKeys: boolean;
}

const layers: EscapeLayer[] = [];

/** The layer Esc closes now: the newest one with no other layer inside it. */
export function topLayer(open: readonly EscapeLayer[] = layers): EscapeLayer | undefined {
  for (let i = open.length - 1; i >= 0; i--) {
    const layer = open[i];
    if (layer === undefined) continue;
    const el = layer.element();
    const covered =
      el !== null &&
      open.some((other) => {
        if (other === layer) return false;
        const inner = other.element();
        return inner !== null && inner !== el && el.contains(inner);
      });
    if (!covered) return layer;
  }
  return undefined;
}

function onKey(e: KeyboardEvent): void {
  if (e.key !== "Escape" || e.isComposing) return;
  const top = topLayer();
  if (top === undefined) return;
  const el = top.element();
  if (top.ownKeys && el !== null && e.target instanceof Node && el.contains(e.target)) return;
  e.preventDefault();
  e.stopImmediatePropagation();
  top.close();
}

/** Opens a layer; the returned function takes it away. */
export function addEscapeLayer(layer: EscapeLayer): () => void {
  if (layers.length === 0) document.addEventListener("keydown", onKey, true);
  layers.push(layer);
  return () => {
    const at = layers.indexOf(layer);
    if (at >= 0) layers.splice(at, 1);
    if (layers.length === 0) document.removeEventListener("keydown", onKey, true);
  };
}

/** Makes the mounted component a layer that Esc closes with `close` (the newest `close` while it stays open). */
export function useEscapeLayer(
  close: () => void,
  element?: RefObject<Element | null>,
  ownKeys = false,
): void {
  const latest = useRef(close);
  useEffect(() => {
    latest.current = close;
  });
  useEffect(
    () =>
      addEscapeLayer({
        close: () => {
          latest.current();
        },
        element: () => element?.current ?? null,
        ownKeys,
      }),
    [element, ownKeys],
  );
}
