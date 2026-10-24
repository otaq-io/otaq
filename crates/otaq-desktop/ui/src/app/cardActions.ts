// A button pressed on a board card (T-0220): the board opens the sheet on that ticket and leaves the action
// here, where the sheet (T-0222 and the tickets after it) picks it up and runs it (Resume, Retry start, an answer
// option, Deliver now, Mark merged…), as the GPUI app's panel did. The newest request only.
import type { CardActionKind } from "../generated/CardActionKind";
import { createStore, type Store } from "./store";

export interface CardActionRequest {
  ticket: string;
  kind: CardActionKind;
  /** Counts up with each request, so pressing the same button twice is two requests. */
  seq: number;
}

export const cardActions: Store<CardActionRequest | null> = createStore<CardActionRequest | null>(null);

export function requestCardAction(
  ticket: string,
  kind: CardActionKind,
  store: Store<CardActionRequest | null> = cardActions,
): void {
  store.update((last) => ({ ticket, kind, seq: (last?.seq ?? 0) + 1 }));
}

/** The sheet showing `ticket` takes the request meant for it, once: it is cleared so a later sheet doesn't run it again. */
export function takeCardAction(
  ticket: string,
  store: Store<CardActionRequest | null> = cardActions,
): CardActionRequest | null {
  const pending = store.get();
  if (pending?.ticket !== ticket) return null;
  store.set(null);
  return pending;
}
