// @vitest-environment happy-dom
// A click on the board's background closes the ticket sheet (OT-3); a click on another card still switches it, and a
// column's button, a drag, a right click or a click that closes a menu leave it open.
import { afterEach, describe, expect, it, vi } from "vitest";

import { closeTicket, createSelection, openTicket } from "../../app/selection";
import { createBackgroundClick, isBoardBackground } from "./background";
import { createDrag } from "./drag";

/** A board with two columns: Backlog with a card and its + button, and an empty Dev. */
function board() {
  document.body.innerHTML = `
    <div class="board">
      <div class="board-columns">
        <section class="board-column" data-column="backlog">
          <header class="board-column-header">
            <span class="board-column-title">Backlog</span>
            <span class="board-spacer"></span>
            <button type="button" class="board-column-button"><svg></svg></button>
          </header>
          <div class="board-column-cards">
            <div data-card="SHOP-24"><span class="card-title">Price table</span></div>
          </div>
        </section>
        <section class="board-column" data-column="dev">
          <div class="board-column-cards"></div>
        </section>
      </div>
    </div>`;
  const $ = (selector: string) => {
    const el = document.querySelector(selector);
    if (el === null) throw new Error(selector);
    return el;
  };
  return {
    cards: $('[data-column="dev"] .board-column-cards'),
    gap: $(".board-columns"),
    card: $(".card-title"),
    button: $(".board-column-button svg"),
    title: $(".board-column-title"),
  };
}

const at = (target: Element, x = 100, y = 100, button = 0) => ({ target, clientX: x, clientY: y, button });

afterEach(() => {
  document.body.innerHTML = "";
});

describe("a click on the board's background (OT-3)", () => {
  it("closes the sheet", () => {
    const el = board();
    const store = createSelection();
    openTicket("SHOP-24", undefined, store);
    const click = createBackgroundClick(() => {
      closeTicket(store);
    });
    click.press(at(el.cards));
    click.release(at(el.cards, 101, 101));
    expect(store.get().ticket).toBeNull();

    openTicket("SHOP-24", undefined, store);
    click.press(at(el.gap));
    click.release(at(el.gap));
    expect(store.get().ticket).toBeNull();
  });

  it("is only the board's own surfaces, not a card, a button or a title", () => {
    const el = board();
    expect(isBoardBackground(el.cards)).toBe(true);
    expect(isBoardBackground(el.gap)).toBe(true);
    expect(isBoardBackground(el.card)).toBe(false);
    expect(isBoardBackground(el.button)).toBe(false);
    expect(isBoardBackground(el.title)).toBe(false);
    expect(isBoardBackground(null)).toBe(false);
  });

  it("a click on another card switches the sheet, not closes it", () => {
    const el = board();
    const close = vi.fn();
    const click = createBackgroundClick(close);
    const drag = createDrag(() => new Promise(() => undefined), { onState: () => undefined });
    click.press(at(el.card));
    drag.press("SHOP-24", 100, 100);
    click.release(at(el.card));
    expect(drag.release()).toEqual({ kind: "click", key: "SHOP-24" });
    expect(close).not.toHaveBeenCalled();
  });

  it("leaves the sheet open on a column's button, a drag, a right click or a cancelled press", () => {
    const el = board();
    const close = vi.fn();
    const click = createBackgroundClick(close);
    // The column's + button.
    click.press(at(el.button));
    click.release(at(el.button));
    // A card dragged and dropped on an empty column.
    click.press(at(el.card));
    click.release(at(el.cards, 400, 100));
    // A drag that starts on empty space.
    click.press(at(el.cards));
    click.release(at(el.cards, 140, 100));
    // A press on empty space released over a card.
    click.press(at(el.cards));
    click.release(at(el.card));
    // A right click.
    click.press(at(el.cards, 100, 100, 2));
    click.release(at(el.cards));
    // A press the window lost (blur, pointercancel).
    click.press(at(el.cards));
    click.cancel();
    click.release(at(el.cards));
    expect(close).not.toHaveBeenCalled();
  });

  it("leaves the sheet open when the click closes an open menu or popover", () => {
    const el = board();
    const close = vi.fn();
    const click = createBackgroundClick(close);
    for (const floating of ["menu", "popover"]) {
      const open = document.createElement("div");
      open.className = floating;
      document.body.append(open);
      click.press(at(el.cards));
      open.remove();
      click.release(at(el.cards));
    }
    expect(close).not.toHaveBeenCalled();
    click.press(at(el.cards));
    click.release(at(el.cards));
    expect(close).toHaveBeenCalledOnce();
  });
});
