// @vitest-environment happy-dom
// Floating layers draw in the overlay host, above every panel (OTAQ-3: the Waits for picker under the expanded Noyan
// panel), unless they are already inside a layer: then they stay in it, so a press in a menu stays inside its popover,
// Esc closes the menu first and a dialog keeps its pickers. With no host (a test, the gallery) they stay in place.
import { act, createElement, type ReactNode, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import { Dialog } from "./Dialog";
import { OVERLAY_HOST_ID } from "./Floating";
import type { Rect } from "./logic";
import { AnchoredMenu } from "./Menu";
import { Popover } from "./Popover";

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const AT: Rect = { left: 10, top: 10, width: 80, height: 20 };

let root: Root | null = null;
let page: HTMLElement | null = null;
let overlay: HTMLElement | null = null;

/** The page as the frame lays it out: the ticket sheet, and the overlay host beside it (`withHost`). */
function mount(withHost: boolean): HTMLElement {
  page = document.body.appendChild(document.createElement("div"));
  const sheet = page.appendChild(document.createElement("div"));
  sheet.className = "frame-sheet";
  if (withHost) {
    overlay = page.appendChild(document.createElement("div"));
    overlay.id = OVERLAY_HOST_ID;
    overlay.className = "frame-layer";
  }
  root = createRoot(sheet);
  return sheet;
}

beforeEach(() => {
  overlay = null;
});

afterEach(() => {
  act(() => root?.unmount());
  page?.remove();
  vi.restoreAllMocks();
});

function render(node: ReactNode): void {
  act(() => root?.render(node));
}

function popover(children: ReactNode, onClose: () => void = () => undefined): ReactNode {
  return createElement(Popover, { label: "Waits for", anchor: AT, onClose, children });
}

function menu(onClose: () => void): ReactNode {
  return createElement(AnchoredMenu, {
    label: "Kind",
    anchor: AT,
    items: [],
    onChoose: () => undefined,
    onClose,
  });
}

function escape(): void {
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }),
    );
  });
}

const live = (selector: string) => document.querySelector(`${selector}:not([data-leaving])`);

test("a popover opened in the ticket sheet draws in the overlay host, not in the sheet", () => {
  const sheet = mount(true);
  render(popover(createElement("span", null, "rows")));
  const shown = live(".popover");
  expect(shown).not.toBeNull();
  expect(overlay?.contains(shown ?? null)).toBe(true);
  expect(sheet.contains(shown ?? null)).toBe(false);
});

test("with no overlay host (a test, the gallery) it stays in place", () => {
  const sheet = mount(false);
  render(popover(createElement("span", null, "rows")));
  expect(sheet.querySelector(".popover")).not.toBeNull();
});

test("a popover in a dialog stays inside the dialog's box", () => {
  mount(true);
  render(
    createElement(Dialog, {
      title: "New ticket",
      footer: null,
      onCancel: () => undefined,
      children: popover(createElement("span", null, "rows")),
    }),
  );
  const box = overlay?.querySelector(".dialog");
  expect(box).not.toBeNull();
  expect(box?.querySelector(".popover")).not.toBeNull();
});

test("a menu opened from a portaled popover stays inside it; a press there keeps the popover; Esc closes the menu, then the popover", () => {
  mount(true);
  const closed: string[] = [];
  function Layers() {
    const [menuOpen, setMenuOpen] = useState(true);
    const [popoverOpen, setPopoverOpen] = useState(true);
    if (!popoverOpen) return null;
    return popover(
      menuOpen &&
        menu(() => {
          closed.push("menu");
          setMenuOpen(false);
        }),
      () => {
        closed.push("popover");
        setPopoverOpen(false);
      },
    );
  }
  render(createElement(Layers));
  const shown = live(".popover");
  const inner = live(".menu");
  expect(overlay?.contains(shown ?? null)).toBe(true);
  expect(shown?.contains(inner ?? null)).toBe(true);
  // Only one floating layer in the host: the menu is inside the popover, not beside it.
  expect(overlay?.querySelectorAll(":scope > .floating")).toHaveLength(0);

  act(() => {
    inner?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  });
  expect(closed).toEqual([]);

  escape();
  expect(closed).toEqual(["menu"]);
  escape();
  expect(closed).toEqual(["menu", "popover"]);
});

test("a closing popover stays in the host through its leave", () => {
  mount(true);
  // The page can animate: a leave that never ends while the test looks.
  const before = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "animate");
  const animate = vi.fn(() => ({ finished: new Promise(() => undefined) }) as unknown as Animation);
  Object.defineProperty(HTMLElement.prototype, "animate", {
    value: animate,
    configurable: true,
    writable: true,
  });
  vi.spyOn(window, "getComputedStyle").mockReturnValue({
    getPropertyValue: (name: string) => (name.startsWith("--motion") ? "120ms" : ""),
  } as unknown as CSSStyleDeclaration);
  try {
    render(popover(createElement("span", null, "rows")));
    render(null);
    expect(live(".popover")).toBeNull();
    const leaving = overlay?.querySelector(".popover[data-leaving]");
    expect(leaving).not.toBeNull();
    expect(leaving?.textContent).toBe("rows");
  } finally {
    if (before) Object.defineProperty(HTMLElement.prototype, "animate", before);
    else delete (HTMLElement.prototype as { animate?: unknown }).animate;
  }
});
