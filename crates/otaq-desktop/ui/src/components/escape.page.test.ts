// @vitest-environment happy-dom
// Esc closes the topmost open menu or popover wherever the focus is (the project switcher and the Pause menu
// included), and only that one: never the ticket under it, nor a popover under a menu.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import { installKeys, keymap } from "../app/keyboard";
import { createRegistries, type Registries } from "../app/registry";
import type { KeyModifiers } from "../generated/KeyModifiers";
import { topLayer } from "./escape";
import type { Rect } from "./logic";
import { AnchoredMenu, Menu } from "./Menu";
import { Popover } from "./Popover";

vi.mock("../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const NONE: KeyModifiers = { control: false, alt: false, shift: false, platform: false, function: false };
const AT: Rect = { left: 10, top: 10, width: 80, height: 20 };

let root: Root | null = null;
let host: HTMLElement | null = null;
let stop: (() => void) | null = null;
let app: Registries;
let ran: string[];

beforeEach(() => {
  app = createRegistries();
  ran = [];
  app.shortcuts.on("close_sheet", () => {
    ran.push("close_sheet");
  });
  stop = installKeys(app);
  keymap.set({
    platform: "mac",
    letters: true,
    log: false,
    typing_hint: "",
    actions: [],
    terminal_keys: [],
    terminal_app_modifiers: [],
    bindings: [
      {
        action: { action: "close_sheet" },
        slot: 0,
        text: "esc",
        key: { kind: "stroke", stroke: { key: "Escape", code: "Escape", modifiers: NONE } },
        place: "outside_terminals",
        custom: false,
      },
    ],
    sequences: [],
  });
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  stop?.();
});

function press(target: Element | null): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key: "Escape",
    code: "Escape",
    bubbles: true,
    cancelable: true,
  });
  act(() => {
    (target ?? document.body).dispatchEvent(event);
  });
  return event;
}

const elsewhere = () => host?.querySelector<HTMLButtonElement>(".elsewhere") ?? null;

function Layers({ closed, menu }: { closed: string[]; menu: boolean }) {
  return createElement(
    "div",
    null,
    createElement("button", { type: "button", className: "elsewhere" }, "Pause"),
    createElement(Popover, {
      label: "Pause agents",
      anchor: AT,
      onClose: () => {
        closed.push("popover");
      },
      children: createElement("span", null, "rows"),
    }),
    menu &&
      createElement(AnchoredMenu, {
        label: "Approve",
        anchor: AT,
        items: [],
        onChoose: () => undefined,
        onClose: () => {
          closed.push("menu");
        },
      }),
  );
}

test("Esc closes an open popover with the focus elsewhere, and not the ticket under it", () => {
  const closed: string[] = [];
  act(() => root?.render(createElement(Layers, { closed, menu: false })));
  act(() => elsewhere()?.focus());
  press(elsewhere());
  expect(closed).toEqual(["popover"]);
  expect(ran).toEqual([]);
  // With nothing focused at all.
  act(() => elsewhere()?.blur());
  press(document.body);
  expect(closed).toEqual(["popover", "popover"]);
  expect(ran).toEqual([]);
});

test("one Esc closes only the topmost layer: the menu over a popover", () => {
  const closed: string[] = [];
  act(() => root?.render(createElement(Layers, { closed, menu: true })));
  press(document.activeElement);
  expect(closed).toEqual(["menu"]);
  // The focus left the menu: Esc still closes it, not the popover under it.
  closed.length = 0;
  press(elsewhere());
  expect(closed).toEqual(["menu"]);
  expect(ran).toEqual([]);
});

test("a menu opened inside a popover is above it, though the popover registered after it", () => {
  const closed: string[] = [];
  function Nested() {
    return createElement(
      "div",
      null,
      createElement("button", { type: "button", className: "elsewhere" }, "shop-web"),
      createElement(Popover, {
        label: "Projects",
        anchor: AT,
        onClose: () => {
          closed.push("popover");
        },
        children: createElement(Menu, {
          label: "Switch project",
          items: [],
          onChoose: () => undefined,
          onClose: () => {
            closed.push("menu");
          },
        }),
      }),
    );
  }
  act(() => root?.render(createElement(Nested)));
  press(elsewhere());
  expect(closed).toEqual(["menu"]);
});

test("the topmost layer is the newest with no other layer inside it", () => {
  const outer = document.createElement("div");
  const inner = outer.appendChild(document.createElement("div"));
  const layer = (el: Element | null) => ({ close: () => undefined, element: () => el, ownKeys: false });
  const a = layer(inner);
  const b = layer(outer);
  const c = layer(null);
  expect(topLayer([a, b])).toBe(a);
  expect(topLayer([a, b, c])).toBe(c);
  expect(topLayer([])).toBeUndefined();
});
