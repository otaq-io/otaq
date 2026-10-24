// @vitest-environment happy-dom
// The ? list in a page (T-0394, KEY-07): `?` opens it while nobody is typing, not in a field; Esc closes it and runs
// nothing behind it; its link opens Settings on Keyboard shortcuts.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import { installKeys, keymap } from "../../app/keyboard";
import { overlays } from "../../app/overlays";
import { createRegistries, type Registries } from "../../app/registry";
import type { KeyBinding } from "../../generated/KeyBinding";
import type { Keymap } from "../../generated/Keymap";
import type { ShortcutListView } from "../../generated/ShortcutListView";
import { shortcuts } from ".";
import { KEYBOARD_SECTION } from "./logic";
import { ShortcutsOverlay } from "./ShortcutList";

const VIEW: ShortcutListView = {
  title: "Keyboard shortcuts",
  settings_link: "Change shortcuts in Settings →",
  close: "close",
  note: "Single letters work while you aren’t typing in a field, the editor, the Project agent’s box or a terminal.",
  or: "or",
  then: "then",
  columns: [
    [
      {
        group: "moving_around",
        title: "Moving around",
        rows: [
          { title: "Show all shortcuts", keys: [{ kbds: ["?"] }], changed: false, off: false, mark: null },
        ],
      },
    ],
    [
      {
        group: "views",
        title: "Views",
        rows: [
          {
            title: "Activity",
            keys: [{ kbds: ["⌘J"] }, { kbds: ["g", "a"] }],
            changed: true,
            off: false,
            mark: "changed",
          },
        ],
      },
    ],
  ],
};

const requestMock = vi.hoisted(() => vi.fn());
vi.mock("../../tauri", () => ({
  request: requestMock,
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const binding = (action: KeyBinding["action"], key: KeyBinding["key"], text: string): KeyBinding => ({
  action,
  slot: 0,
  text,
  key,
  place: "outside_terminals",
  custom: false,
});

const MAP: Keymap = {
  platform: "mac",
  letters: true,
  log: false,
  typing_hint: "Typing? Click where you want to type",
  actions: [],
  terminal_keys: [],
  terminal_app_modifiers: [],
  sequences: [],
  bindings: [
    binding({ action: "show_shortcuts" }, { kind: "letter", letter: "?" }, "?"),
    binding(
      { action: "close_panel" },
      {
        kind: "stroke",
        stroke: {
          key: "Escape",
          code: "Escape",
          modifiers: { control: false, alt: false, shift: false, platform: false, function: false },
        },
      },
      "Esc",
    ),
  ],
};

let root: Root | null = null;
let host: HTMLElement | null = null;
let stop: (() => void) | null = null;
let app: Registries;
let ran: string[];

beforeEach(() => {
  requestMock.mockImplementation((method: string) =>
    method === "shortcuts.list" ? Promise.resolve(VIEW) : new Promise(() => undefined),
  );
  app = createRegistries();
  shortcuts(app);
  ran = [];
  app.shortcuts.on("close_panel", () => {
    ran.push("close_panel");
  });
  stop = installKeys(app);
  keymap.set(MAP);
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  stop?.();
  act(() => {
    overlays.store.set([]);
  });
});

function press(key: string): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key,
    code: key,
    bubbles: true,
    cancelable: true,
    shiftKey: key === "?",
  });
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(event);
  });
  return event;
}

const open = () => overlays.store.get().map((o) => o.id);

/** The overlay as the frame draws it, closing itself through the stack. */
async function renderOverlay(): Promise<void> {
  await act(async () => {
    root?.render(
      createElement(ShortcutsOverlay, {
        params: undefined,
        close: () => {
          overlays.close("shortcuts");
        },
      }),
    );
    await Promise.resolve();
  });
}

test("? opens the list while nobody is typing", () => {
  expect(press("?").defaultPrevented).toBe(true);
  expect(open()).toEqual(["shortcuts"]);
});

test("? types into a field and opens nothing", () => {
  act(() => root?.render(createElement("input")));
  act(() => host?.querySelector("input")?.focus());
  expect(press("?").defaultPrevented).toBe(false);
  expect(open()).toEqual([]);
});

test("the list shows Rust's rows; Esc closes it and runs nothing behind it", async () => {
  press("?");
  await renderOverlay();
  expect(requestMock).toHaveBeenCalledWith("shortcuts.list");
  const box = host?.querySelector('[role="dialog"]');
  expect(box?.textContent).toContain("Keyboard shortcuts");
  expect(host?.querySelector(".shortcuts-row[data-changed]")?.textContent).toBe("Activity⌘Jorgthenachanged");
  expect(document.activeElement).toBe(box);
  // A letter doesn't act behind it, nor a second ? a moment later (one at once would be a burst, d0126).
  const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + 1000);
  press("?");
  later.mockRestore();
  expect(open()).toEqual(["shortcuts"]);
  press("Escape");
  expect(open()).toEqual([]);
  expect(ran).toEqual([]);
});

test("the link opens Settings on Keyboard shortcuts", async () => {
  press("?");
  await renderOverlay();
  const link = [...(host?.querySelectorAll("button") ?? [])].find((b) =>
    b.textContent.includes("Change shortcuts in Settings"),
  );
  act(() => link?.click());
  expect(overlays.store.get()).toEqual([{ id: "settings", params: { section: KEYBOARD_SECTION } }]);
  expect(KEYBOARD_SECTION).toBe("keyboard_shortcuts");
});
