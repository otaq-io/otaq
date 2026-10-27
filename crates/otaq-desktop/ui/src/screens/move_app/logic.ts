// What the Move window does with `move_app.move`'s answer (T-0579): `{ moved: true }` once the moved copy is open,
// so this one quits; otherwise the window in its failure state.
import type { MoveAppView } from "../../generated/MoveAppView";

export type MoveOutcome = { quit: true } | { quit: false; view: MoveAppView };

export function afterMove(answer: unknown): MoveOutcome {
  if (typeof answer === "object" && answer !== null && "moved" in answer && answer.moved === true) {
    return { quit: true };
  }
  return { quit: false, view: answer as MoveAppView };
}
