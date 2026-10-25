// How the cards turn the view model's unions into colours and states (T-0218). Every union the card draws has an
// exhaustive switch here (the linter's switch-exhaustiveness-check fails on a missing case), and each list of
// variants below is checked against its union, so a variant added in Rust can't go undrawn.
import type { CardLook } from "../generated/CardLook";
import type { CardTone } from "../generated/CardTone";
import type { PillPalette } from "../generated/PillPalette";

/** A tone's ink and its soft background, as CSS colours from the theme's variables. */
export interface Ink {
  fg: string;
  bg: string;
}

export function toneInk(tone: CardTone): Ink {
  switch (tone) {
    case "muted":
      return { fg: "var(--text-muted)", bg: "var(--bg-column)" };
    case "attention":
      return { fg: "var(--st-attn)", bg: "var(--st-attn-bg)" };
    case "error":
      return { fg: "var(--st-error)", bg: "var(--st-error-bg)" };
    case "warning":
      return { fg: "var(--st-paused)", bg: "var(--st-paused-bg)" };
    case "working":
      return { fg: "var(--st-working)", bg: "var(--st-working-bg)" };
    case "review":
      return { fg: "var(--st-review)", bg: "var(--st-review-bg)" };
    case "epic":
      return { fg: "var(--st-investigate)", bg: "var(--st-investigate-bg)" };
    case "accent":
      return { fg: "var(--accent)", bg: "var(--accent-soft)" };
    case "secondary":
      return { fg: "var(--text-secondary)", bg: "var(--bg-column)" };
    // FLW-14: how it passed (a green edge) and a skip or a move by you (an amber edge), with calm text.
    case "passed":
      return { fg: "var(--st-approved)", bg: "var(--st-approved-bg)" };
    case "paused":
      return { fg: "var(--st-paused)", bg: "var(--st-paused-bg)" };
  }
}

/** The coloured edge of a card that needs the user (BRD-03): its border and its 3 px left side. */
export function edgeColor(tone: CardTone): string {
  return toneInk(tone).fg;
}

/** A status line's dot: the state's colour (the status pills' pairs, `components.css`). */
export function paletteInk(palette: PillPalette): string {
  switch (palette) {
    case "attn":
      return "var(--st-attn)";
    case "error":
      return "var(--st-error)";
    case "review":
      return "var(--st-review)";
    case "paused":
      return "var(--st-paused)";
    case "working":
      return "var(--st-working)";
    case "queued":
      return "var(--st-queued)";
    case "approved":
      return "var(--st-approved)";
    case "done":
      return "var(--st-done)";
    case "epic":
      return "var(--st-investigate)";
    case "pinned":
      return "var(--accent)";
  }
}

/** How a card relates to the dependency chain being highlighted (DEP-05): the hovered or selected card itself
 * (`origin`, the frame's 2 px accent outline), another member, or a card outside the chain. */
export type ChainState = "none" | "origin" | "highlighted" | "dimmed";

/** The moment of interaction a card is drawn in; the board decides it, the gallery draws each on purpose. */
export interface CardState {
  selected: boolean;
  chain: ChainState;
  dragging: boolean;
  /** Draws the hovered look (and the quick actions) without the pointer. */
  hovered: boolean;
}

export const RESTING: CardState = { selected: false, chain: "none", dragging: false, hovered: false };

/** The gallery's variant look as a card state. */
export function stateOfLook(look: CardLook): CardState {
  switch (look) {
    case "plain":
      return RESTING;
    case "selected":
      return { ...RESTING, selected: true };
    case "hovered":
      return { ...RESTING, hovered: true };
    case "dragging":
      return { ...RESTING, dragging: true };
    case "chain":
      return { ...RESTING, chain: "highlighted" };
    case "dimmed":
      return { ...RESTING, chain: "dimmed" };
  }
}

/** Every variant of a union, checked both ways: no unknown entry, none missing. */
type Every<U extends string, L extends readonly U[]> = [Exclude<U, L[number]>] extends [never] ? L : never;

const every =
  <U extends string>() =>
  <L extends readonly U[]>(list: Every<U, L>) =>
    list;

export const TONES = every<CardTone>()([
  "muted",
  "attention",
  "error",
  "warning",
  "working",
  "review",
  "epic",
  "accent",
  "secondary",
  "passed",
  "paused",
] as const);
export const PALETTES = every<PillPalette>()([
  "attn",
  "error",
  "review",
  "paused",
  "working",
  "queued",
  "approved",
  "done",
  "epic",
  "pinned",
] as const);
export const LOOKS = every<CardLook>()([
  "plain",
  "selected",
  "hovered",
  "dragging",
  "chain",
  "dimmed",
] as const);
