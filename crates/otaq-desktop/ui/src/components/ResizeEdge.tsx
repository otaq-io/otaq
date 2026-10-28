// A dragged edge (T-0502): the Project agent panel's right edge, the ticket sheet's two edges, the Files tree's right
// edge. Dragging it reports the width it gives (`onDrag`, kept inside `clamp`); letting go keeps that width
// (`onKeep`), and a double-click goes back to the default (`onKeep(null)`). The drag itself stays on the page.
import { type CSSProperties, type PointerEvent as ReactPointerEvent, useRef } from "react";

import { resizedWidth } from "./logic";

export interface ResizeEdgeProps {
  /** Which side of its box the edge is on: a right edge widens the box dragged rightwards, a left edge leftwards. */
  side: "left" | "right";
  /** The box is centred, so it grows on both sides: twice the pointer's move, and the edge stays under it. */
  centered?: boolean;
  /** The box's width now; without it, the edge's parent is measured when the drag starts. */
  width?: number;
  /** Keeps a dragged width inside the box's bounds. */
  clamp: (width: number) => number;
  /** The width during a drag; `null` once the kept width is back. */
  onDrag: (width: number | null) => void;
  /** Keeps the width the drag ended at (`null`: the double-click, back to the default). */
  onKeep: (width: number | null) => Promise<void>;
  /** The tooltip (Rust's `layout::EDGE`). */
  title: string;
  /** What the console names when the width can't be kept ("the panel's width"). */
  name: string;
  className?: string;
  style?: CSSProperties;
}

export function ResizeEdge({
  side,
  centered = false,
  width,
  clamp,
  onDrag,
  onKeep,
  title,
  name,
  className,
  style,
}: ResizeEdgeProps) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const dragged = useRef<number | null>(null);
  const keep = (kept: number | null) => {
    onKeep(kept).then(
      () => {
        onDrag(null);
      },
      (error: unknown) => {
        onDrag(null);
        console.error(`${name} couldn't be kept`, error);
      },
    );
  };
  const down = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    const measured = e.currentTarget.parentElement?.getBoundingClientRect().width ?? 0;
    start.current = { x: e.clientX, width: width ?? measured };
    dragged.current = null;
  };
  const move = (e: ReactPointerEvent<HTMLDivElement>) => {
    const from = start.current;
    if (!from) return;
    dragged.current = clamp(resizedWidth(from.width, e.clientX - from.x, side, centered));
    onDrag(dragged.current);
  };
  const up = () => {
    const kept = dragged.current;
    start.current = null;
    dragged.current = null;
    if (kept !== null) keep(kept);
  };
  return (
    <div
      className={className ? `resize-edge ${className}` : "resize-edge"}
      data-side={side}
      // A pointer's handle, as the flow editor's grips: a width has no key of its own.
      aria-hidden="true"
      title={title}
      style={style}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onDoubleClick={() => {
        keep(null);
      }}
    />
  );
}
