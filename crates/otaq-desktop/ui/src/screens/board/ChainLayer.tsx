// The dependencies' drawing layer over the board (DPV-02, d0006; *24 · Dependencies on the board* `s24-09`, *Always*
// `s24-10`): with Dependencies: Always, every open link between cards in view, quiet and dashed; over them the hovered
// or selected card's chain in accent, parent → child, and under a hovered linked card its tooltip ("SHOP-20 is
// blocked" · "by SHOP-18 (Review) and SHOP-29 (Dev)"). Only cards in view are joined (a card scrolled out of its
// column, folded in Dev or in the Done rail gets none). Like the lifted card it is fixed to the window inside the
// scrolling board, laid over the board's own box, and measured again as the board or a column scrolls or resizes.
import { useLayoutEffect, useRef } from "react";

import type { ChainEdge } from "../../generated/ChainEdge";
import type { ChainView } from "../../generated/ChainView";
import { type Arrow, arrow, type Box, inView } from "./chain";

/** The tooltip's gap under its card. */
const TIP_GAP = 6;

const SVG = "http://www.w3.org/2000/svg";

function path(className: string, d: string): SVGPathElement {
  const el = document.createElementNS(SVG, "path");
  el.setAttribute("class", className);
  el.setAttribute("d", d);
  return el;
}

function relative(r: DOMRect, origin: DOMRect): Box {
  return {
    left: r.left - origin.left,
    top: r.top - origin.top,
    right: r.right - origin.left,
    bottom: r.bottom - origin.top,
  };
}

/** The boxes of the cards `keys` that are in view inside `board`, in the board box's coordinates. */
function boxesOf(keys: Iterable<string>, board: HTMLElement, origin: DOMRect): Map<string, Box> {
  const view = relative(origin, origin);
  const boxes = new Map<string, Box>();
  for (const key of keys) {
    if (boxes.has(key)) continue;
    const card = board.querySelector<HTMLElement>(`[data-card="${CSS.escape(key)}"]`);
    if (!card) continue;
    const box = relative(card.getBoundingClientRect(), origin);
    const scroller = card.closest<HTMLElement>(".board-column-cards");
    const clips = scroller ? [view, relative(scroller.getBoundingClientRect(), origin)] : [view];
    if (inView(box, clips)) boxes.set(key, box);
  }
  return boxes;
}

/** The arrows of `edges` whose two cards have a box. */
export function arrowsOf(edges: readonly ChainEdge[], boxes: ReadonlyMap<string, Box>): Arrow[] {
  return edges.flatMap(({ parent, child }) => {
    const from = boxes.get(parent);
    const to = boxes.get(child);
    return from && to ? [arrow(from, to)] : [];
  });
}

/** Always's links without those the chain shown draws itself. */
export function quietLinks(links: readonly ChainEdge[], chain: ChainView | null): ChainEdge[] {
  if (chain === null) return [...links];
  return links.filter((l) => !chain.edges.some((e) => e.parent === l.parent && e.child === l.child));
}

const px = (n: number) => `${String(Math.round(n))}px`;

export interface ChainLayerProps {
  chain: ChainView | null;
  /** Dependencies: Always's links (`BoardView.links`); empty with On hover. */
  links: readonly ChainEdge[];
  /** The hovered card: its chain's tooltip shows under it. */
  hovered: string | null;
  /** The board's scrolling element, holding the cards. */
  board: HTMLElement | null;
  /** Changes whenever the cards may have moved (a new board view). */
  layout: unknown;
}

export function ChainLayer({ chain, links, hovered, board, layout }: ChainLayerProps) {
  const svg = useRef<SVGSVGElement>(null);
  const tipBox = useRef<HTMLDivElement>(null);
  const tip = chain !== null && chain.key === hovered ? chain.tip : null;
  const drawn = chain !== null || links.length > 0;

  // The layer follows the cards' places on screen, so it is placed and drawn straight in the DOM (no render per
  // scroll).
  useLayoutEffect(() => {
    const layer = svg.current;
    if (!drawn || board === null || layer === null) return;
    let pending = 0;
    const quiet = quietLinks(links, chain);
    const draw = () => {
      const box = board.getBoundingClientRect();
      Object.assign(layer.style, {
        left: px(box.left),
        top: px(box.top),
        width: px(box.width),
        height: px(box.height),
      });
      const keys = [...(chain?.members ?? []), ...quiet.flatMap((l) => [l.parent, l.child])];
      const boxes = boxesOf(keys, board, box);
      layer.replaceChildren(
        ...arrowsOf(quiet, boxes).flatMap((a) => [
          path("board-link-line", a.line),
          path("board-link-head", a.head),
        ]),
        ...(chain === null ? [] : arrowsOf(chain.edges, boxes)).flatMap((a) => [
          path("board-chain-line", a.line),
          path("board-chain-head", a.head),
        ]),
      );
      const note = tipBox.current;
      const under = chain === null ? undefined : boxes.get(chain.key);
      if (note) {
        note.hidden = under === undefined;
        if (under) {
          Object.assign(note.style, {
            left: px(box.left + under.left),
            top: px(box.top + under.bottom + TIP_GAP),
          });
        }
      }
    };
    const later = () => {
      cancelAnimationFrame(pending);
      pending = requestAnimationFrame(draw);
    };
    draw();
    // Scroll events don't bubble; capturing them on the board catches the board's and every column's.
    board.addEventListener("scroll", later, { capture: true, passive: true });
    const observer = new ResizeObserver(later);
    observer.observe(board);
    window.addEventListener("resize", later);
    return () => {
      cancelAnimationFrame(pending);
      board.removeEventListener("scroll", later, { capture: true });
      observer.disconnect();
      window.removeEventListener("resize", later);
    };
  }, [chain, links, drawn, board, layout, tip]);

  if (!drawn) return null;
  return (
    <>
      <svg className="board-chain-arrows" ref={svg} aria-hidden="true" />
      {tip !== null && (
        <div className="tooltip board-chain-tip" data-rich ref={tipBox} role="status" hidden>
          <strong>{tip.title}</strong>
          <span>{tip.detail}</span>
        </div>
      )}
    </>
  );
}
