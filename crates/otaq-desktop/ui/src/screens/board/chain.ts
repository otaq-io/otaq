// The chain highlight's moment of interaction (DEP-05, T-0229; *03 → Chain highlight · hover or select SHOP-20*):
// whose chain shows (the hovered card's, else the selected one's; none while dragging), each card's look, and the arrows
// between members that are in view. Which cards belong to a chain is Rust's (`BoardView.chains`, sent up front).
import type { ChainState } from "../../cards";
import type { ChainView } from "../../generated/ChainView";

/** The chain shown: the hovered card's, else the selected card's; nothing while a card is dragged. */
export function activeChain(
  chains: readonly ChainView[],
  hovered: string | null,
  selected: string | null,
  dragging: boolean,
): ChainView | null {
  if (dragging) return null;
  const shown = hovered ?? selected;
  return shown === null ? null : (chains.find((c) => c.key === shown) ?? null);
}

/** A card's look under the chain shown. */
export function chainStateOf(chain: ChainView | null, key: string): ChainState {
  if (chain === null) return "none";
  if (chain.key === key) return "origin";
  return chain.members.includes(key) ? "highlighted" : "dimmed";
}

/** The members among `keys` (a column's cards, the Done rail's, the cards behind Dev's fold). */
export function membersIn(chain: ChainView | null, keys: readonly string[]): string[] {
  return chain === null ? [] : keys.filter((k) => chain.members.includes(k));
}

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Whether a card's centre is inside every clip box (its column's scroll area and the board's view). */
export function inView(card: Box, clips: readonly Box[]): boolean {
  const x = (card.left + card.right) / 2;
  const y = (card.top + card.bottom) / 2;
  return clips.every((c) => x >= c.left && x <= c.right && y >= c.top && y <= c.bottom);
}

export interface Arrow {
  /** The stroke, parent → child. */
  line: string;
  /** The filled head at the child's edge. */
  head: string;
}

const round = (n: number) => String(Math.round(n * 10) / 10);
const at = (x: number, y: number) => `${round(x)} ${round(y)}`;

/** How far a same-column arrow bows out past the cards' left edge (the frame's SHOP-20 → SHOP-21). */
const SAME_COLUMN_BOW = 40;
/** The head's length and half width (the frame's 8 × 8 triangle). */
const HEAD_LENGTH = 8;
const HEAD_HALF = 4;

/**
 * The arrow from `parent` to `child`, as the frame draws them: from the parent's side facing the child to the
 * child's near side, a curve that leaves and arrives level; cards in one column join around their left edge.
 */
export function arrow(parent: Box, child: Box): Arrow {
  const py = (parent.top + parent.bottom) / 2;
  const cy = (child.top + child.bottom) / 2;
  const childCentre = (child.left + child.right) / 2;
  let start: [number, number];
  let end: [number, number];
  let a: [number, number];
  let b: [number, number];
  if (childCentre > parent.right) {
    start = [parent.right, py];
    end = [child.left, cy];
    const dx = (end[0] - start[0]) / 2;
    a = [start[0] + dx, py];
    b = [end[0] - dx, cy];
  } else if (childCentre < parent.left) {
    start = [parent.left, py];
    end = [child.right, cy];
    const dx = (start[0] - end[0]) / 2;
    a = [start[0] - dx, py];
    b = [end[0] + dx, cy];
  } else {
    start = [parent.left, py];
    end = [child.left, cy];
    a = [start[0] - SAME_COLUMN_BOW, py];
    b = [end[0] - SAME_COLUMN_BOW, cy];
  }
  // The head points from the last control point into the child; the line stops at the head's base.
  const dx = end[0] - b[0];
  const dy = end[1] - b[1];
  const length = Math.max(Math.hypot(dx, dy), 0.001);
  const ux = dx / length;
  const uy = dy / length;
  const base: [number, number] = [end[0] - ux * HEAD_LENGTH, end[1] - uy * HEAD_LENGTH];
  const side = (s: number) => at(base[0] - uy * HEAD_HALF * s, base[1] + ux * HEAD_HALF * s);
  return {
    line: `M${at(...start)} C${at(...a)} ${at(...b)} ${at(...base)}`,
    head: `M${at(...end)} L${side(1)} L${side(-1)} Z`,
  };
}
