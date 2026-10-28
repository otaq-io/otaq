// @vitest-environment happy-dom
// A click on the backdrop around the ticket sheet closes it; a click on the sheet, a right click, a press that ends on
// the sheet or a click that closes a menu leave it open.
import { afterEach, describe, expect, it, vi } from "vitest";

import { closeTicket, createSelection, openTicket } from "./selection";
import { createScrimClick } from "./scrim";

/** The frame's sheet layer with the sheet and a button inside it. */
function layer() {
  document.body.innerHTML = `
    <div class="frame-sheet">
      <div class="sheet"><button type="button">Close</button></div>
    </div>`;
  const $ = (selector: string) => {
    const el = document.querySelector(selector);
    if (el === null) throw new Error(selector);
    return el;
  };
  return { scrim: $(".frame-sheet"), sheet: $(".sheet"), button: $(".sheet button") };
}

const on = (target: Element, scrim: Element, button = 0) => ({ target, currentTarget: scrim, button });

afterEach(() => {
  document.body.innerHTML = "";
});

describe("a click on the sheet's backdrop", () => {
  it("closes the sheet", () => {
    const el = layer();
    const store = createSelection();
    openTicket("SHOP-24", undefined, store);
    const click = createScrimClick(() => {
      closeTicket(store);
    });
    click.press(on(el.scrim, el.scrim));
    click.release(on(el.scrim, el.scrim));
    expect(store.get().ticket).toBeNull();
  });

  it("leaves the sheet open on the sheet, a right click or a press that ends elsewhere", () => {
    const el = layer();
    const close = vi.fn();
    const click = createScrimClick(close);
    // A click inside the sheet.
    click.press(on(el.button, el.scrim));
    click.release(on(el.button, el.scrim));
    // A press inside the sheet released on the backdrop (a text selection dragged out).
    click.press(on(el.sheet, el.scrim));
    click.release(on(el.scrim, el.scrim));
    // A press on the backdrop released on the sheet.
    click.press(on(el.scrim, el.scrim));
    click.release(on(el.sheet, el.scrim));
    // A right click.
    click.press(on(el.scrim, el.scrim, 2));
    click.release(on(el.scrim, el.scrim));
    expect(close).not.toHaveBeenCalled();
  });

  it("leaves the sheet open when the click closes an open menu or popover", () => {
    const el = layer();
    const close = vi.fn();
    const click = createScrimClick(close);
    for (const floating of ["menu", "popover"]) {
      const open = document.createElement("div");
      open.className = floating;
      document.body.append(open);
      click.press(on(el.scrim, el.scrim));
      open.remove();
      click.release(on(el.scrim, el.scrim));
    }
    expect(close).not.toHaveBeenCalled();
    click.press(on(el.scrim, el.scrim));
    click.release(on(el.scrim, el.scrim));
    expect(close).toHaveBeenCalledOnce();
  });
});
