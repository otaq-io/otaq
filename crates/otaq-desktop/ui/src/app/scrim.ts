// The dimmed area around the open ticket sheet is a dialog's backdrop: it takes every click over the board and the
// List (the cards, the rows, their buttons, a folded column's rail, the toolbar, the strip), and a plain left click on
// it closes the sheet and does nothing else. Only a press and its release both on the backdrop count, and not one made
// while a menu or popover is open (that click is the one that closes it). Replaces the board's background click (OT-3),
// which left the cards and buttons under the sheet usable and missed the List.

/** What closes itself on a press outside it, so the press is taken by it. */
const FLOATING = ".menu, .popover";

interface Press {
  button: number;
  target: EventTarget | null;
  currentTarget: EventTarget | null;
}

export interface ScrimClick {
  press(e: Press): void;
  /** Calls `close` when the press was a plain click on the backdrop. */
  release(e: Pick<Press, "target" | "currentTarget">): void;
}

export function createScrimClick(close: () => void): ScrimClick {
  let pressed = false;
  return {
    press(e) {
      const floating = e.target instanceof Node && e.target.ownerDocument?.querySelector(FLOATING);
      pressed = e.button === 0 && e.target === e.currentTarget && !floating;
    },
    release(e) {
      const at = pressed;
      pressed = false;
      if (at && e.target === e.currentTarget) close();
    },
  };
}
