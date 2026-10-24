// @vitest-environment happy-dom
// The command palette (T-0393, KEY-06): ⌘K opens it from the board, the panel and a focused terminal; ↑ ↓ skip a
// greyed row, ↵ runs or opens, Esc closes and gives the keyboard back; picking another project's ticket switches
// there and opens its panel; uses from anywhere go to the recent items.
import { act, createElement, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import { installKeys, keymap } from "../../app/keyboard";
import { closeOverlay, overlays, useOpenOverlays } from "../../app/overlays";
import {
  createRegistries,
  type OverlayId,
  type OverlayProps,
  type Registries,
  RegistriesContext,
} from "../../app/registry";
import {
  createSelection,
  focusTerminal,
  NOTHING_SELECTED,
  openTicket,
  selectProject,
  selection,
} from "../../app/selection";
import type { Keymap } from "../../generated/Keymap";
import type { KeyModifiers } from "../../generated/KeyModifiers";
import type { PaletteRow } from "../../generated/PaletteRow";
import type { PaletteUse } from "../../generated/PaletteUse";
import type { PaletteView } from "../../generated/PaletteView";
import { request } from "../../tauri";
import { palette, recordSelection } from ".";
import { firstPickable, joinQuery, paletteKey, splitQuery, stepPickable, usesOf } from "./logic";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const action = (title: string, name: "approve" | "park" | "new_ticket", pickable = true): PaletteRow => ({
  item: {
    kind: "action",
    action: { action: name },
    icon: "check",
    title,
    keys: [],
    reason: pickable ? null : "· no ticket in review",
  },
  pickable,
});
const ticketRow = (key: string, project: string): PaletteRow => ({
  item: { kind: "ticket", key, title: key, project, other_project: null, place: "Dev" },
  pickable: true,
});
const DIVIDER: PaletteRow = { item: { kind: "divider", label: "Other projects" }, pickable: false };

function paletteView(rows: PaletteRow[]): PaletteView {
  return {
    placeholder: "Search tickets, projects and actions…",
    hint_key: ">",
    hint: "actions only",
    actions_only: null,
    section: null,
    rows,
    nothing: null,
    footer: {
      move_keys: ["↑", "↓"],
      move_label: "move",
      run_key: "↵",
      run: "run",
      open: "open",
      close_key: "Esc",
      close: "close",
      opens: "⌘K · ⇧⌘P",
    },
  };
}

test("the highlight skips greyed rows and the divider, wrapping", () => {
  const rows = [
    action("Approve ▾", "approve", false),
    ticketRow("NOTE-1", "NOTE"),
    DIVIDER,
    ticketRow("API-1", "API"),
  ];
  expect(firstPickable(rows)).toBe(1);
  expect(stepPickable(rows, 1, 1)).toBe(3);
  expect(stepPickable(rows, 3, 1)).toBe(1);
  expect(stepPickable(rows, 1, -1)).toBe(3);
  expect(firstPickable([action("Approve ▾", "approve", false)])).toBe(-1);
  expect(stepPickable([action("Approve ▾", "approve", false)], -1, 1)).toBe(-1);
});

test("the field's keys", () => {
  const rows = [action("Approve ▾", "approve", false), action("Park", "park")];
  const field = { value: "a", actionsOnly: false };
  expect(paletteKey("ArrowDown", 1, rows, field)).toEqual({ kind: "move", index: 1 });
  expect(paletteKey("Enter", 1, rows, field)).toEqual({ kind: "choose", index: 1 });
  expect(paletteKey("Enter", 0, rows, field)).toEqual({ kind: "pass" });
  expect(paletteKey("Escape", 0, rows, field)).toEqual({ kind: "close" });
  expect(paletteKey("Backspace", 0, rows, field)).toEqual({ kind: "pass" });
  expect(paletteKey("Backspace", 0, rows, { value: "", actionsOnly: true })).toEqual({ kind: "drop_prefix" });
  expect(splitQuery(">park")).toEqual({ actionsOnly: true, value: "park" });
  expect(splitQuery("park")).toEqual({ actionsOnly: false, value: "park" });
  expect(joinQuery(true, "pa")).toBe(">pa");
});

test("a selection change records the ticket opened or the project switched to", () => {
  const at = (project: string | null, ticket: string | null) => ({ ...NOTHING_SELECTED, project, ticket });
  expect(usesOf(at("NOTE", null), at("NOTE", "NOTE-2"))).toEqual({ kind: "ticket", key: "NOTE-2" });
  expect(usesOf(at("NOTE", "NOTE-2"), at("API", "API-1"))).toEqual({ kind: "project", project: "API" });
  expect(usesOf(at(null, null), at("NOTE", null))).toBeNull();
  expect(usesOf(at("NOTE", "NOTE-2"), at("NOTE", null))).toBeNull();
  expect(usesOf(at("NOTE", "NOTE-2"), at("NOTE", "NOTE-2"))).toBeNull();

  const store = createSelection({ ...NOTHING_SELECTED, project: "NOTE" });
  const recorded: PaletteUse[] = [];
  const stop = recordSelection(store, (u) => recorded.push(u));
  selectProject("API", store);
  openTicket("API-1", undefined, store);
  stop();
  openTicket("API-2", undefined, store);
  expect(recorded).toEqual([
    { kind: "project", project: "API" },
    { kind: "ticket", key: "API-1" },
  ]);
});

// ---- In a page ----

const NONE: KeyModifiers = { control: false, alt: false, shift: false, platform: false, function: false };
const mac = /Mac|iP(hone|[oa]d)/.test(navigator.platform);
const MOD = mac ? { ...NONE, platform: true } : { ...NONE, control: true };

const MAP: Keymap = {
  platform: mac ? "mac" : "win_linux",
  letters: true,
  log: false,
  typing_hint: "Typing? Click where you want to type",
  actions: [],
  terminal_keys: [],
  terminal_app_modifiers: [MOD],
  sequences: [],
  bindings: [
    {
      action: { action: "command_palette" },
      slot: 0,
      text: "⌘K",
      key: { kind: "stroke", stroke: { key: "k", code: "KeyK", modifiers: MOD } },
      place: "everywhere",
      custom: false,
    },
  ],
};

let app: Registries;
let stop: () => void;
let root: Root;
let host: HTMLElement;
let ran: string[];
const ask = vi.mocked(request);

function Overlays(): ReactNode {
  const open = useOpenOverlays();
  return open.map((o) => {
    const entry = app.overlays.get(o.id);
    if (!entry) return null;
    const Component = entry.Component as (props: OverlayProps<OverlayId>) => ReactNode;
    return createElement(Component, {
      key: o.id,
      params: o.params,
      close: () => {
        closeOverlay(o.id);
      },
    });
  });
}

/** The page: a board card, a panel button and a terminal's textarea, each focusable, and the overlays. */
function page() {
  host = document.body.appendChild(document.createElement("div"));
  host.innerHTML = `
    <div class="board"><div class="card" tabindex="0">NOTE-2</div></div>
    <div class="panel"><button>Edit</button></div>
    <div class="xterm"><textarea class="xterm-helper-textarea"></textarea></div>
    <div id="layer"></div>`;
  root = createRoot(host.querySelector("#layer") as HTMLElement);
  act(() => {
    root.render(createElement(RegistriesContext.Provider, { value: app }, createElement(Overlays)));
  });
}

function press(key: string, init: KeyboardEventInit = {}) {
  const target = document.activeElement ?? document.body;
  act(() => {
    target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
  });
}

function commandK() {
  press("k", { code: "KeyK", metaKey: MOD.platform, ctrlKey: MOD.control });
}

const isOpen = () => overlays.store.get().some((o) => o.id === "command_palette");

/** `palette.view` answers `rows` for any query; `palette.record` answers at once. */
function answers(rows: PaletteRow[]) {
  ask.mockImplementation((method: string) =>
    Promise.resolve(method === "palette.view" ? paletteView(rows) : null),
  );
}

/** Lets the answers arrive. */
async function settle() {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  ask.mockReset();
  ask.mockImplementation(() => new Promise(() => undefined));
  app = createRegistries();
  ran = [];
  for (const name of ["approve", "park", "new_ticket"] as const)
    app.shortcuts.on(name, () => {
      ran.push(name);
    });
  palette(app);
  stop = installKeys(app);
  keymap.set(MAP);
  selection.set({ ...NOTHING_SELECTED, project: "NOTE" });
  page();
});

afterEach(() => {
  act(() => {
    overlays.store.set([]);
  });
  act(() => {
    root.unmount();
  });
  host.remove();
  stop();
});

test("⌘K opens it from the board, the panel and a focused terminal", async () => {
  answers([action("Park", "park")]);
  for (const place of [".card", ".panel button"]) {
    (host.querySelector(place) as HTMLElement).focus();
    commandK();
    expect(isOpen(), place).toBe(true);
    expect(document.activeElement?.tagName).toBe("INPUT");
    press("Escape");
    expect(isOpen()).toBe(false);
    expect(document.activeElement).toBe(host.querySelector(place));
  }
  const textarea = host.querySelector("textarea") as HTMLTextAreaElement;
  textarea.focus();
  act(() => {
    focusTerminal("s1");
  });
  commandK();
  expect(overlays.store.get()).toEqual([{ id: "command_palette", params: { terminal: true } }]);
  await settle();
  expect(ask).toHaveBeenCalledWith("palette.view", {
    query: "",
    project: "NOTE",
    ticket: null,
    terminal: true,
  });
});

test("↑ ↓ skip a greyed row, ↵ runs the highlighted action and records it, Esc closes", async () => {
  answers([
    action("Approve ▾", "approve", false),
    action("Park", "park"),
    action("New ticket", "new_ticket"),
  ]);
  (host.querySelector(".card") as HTMLElement).focus();
  commandK();
  await settle();
  const highlighted = () => host.querySelector('[aria-selected="true"]')?.textContent;
  expect(highlighted()).toContain("Park");
  press("ArrowDown");
  expect(highlighted()).toContain("New ticket");
  press("ArrowDown");
  expect(highlighted()).toContain("Park");
  press("ArrowUp");
  expect(highlighted()).toContain("New ticket");
  press("ArrowUp");
  expect(highlighted()).toContain("Park");
  expect(host.querySelector('[aria-disabled="true"]')?.textContent).toContain("· no ticket in review");
  // ↵ on the greyed row (clicked) does nothing.
  act(() => {
    (host.querySelector('[aria-disabled="true"]') as HTMLElement).click();
  });
  expect(ran).toEqual([]);
  press("Enter");
  expect(isOpen()).toBe(false);
  expect(ran).toEqual(["park"]);
  expect(document.activeElement).toBe(host.querySelector(".card"));
  expect(ask).toHaveBeenCalledWith("palette.record", { kind: "action", action: { action: "park" } });
});

test("picking API-1 from NOTE switches project and opens the panel", async () => {
  answers([ticketRow("NOTE-2", "NOTE"), DIVIDER, ticketRow("API-1", "API")]);
  commandK();
  await settle();
  expect(host.querySelector(".palette-footer")?.textContent).toContain("open");
  press("ArrowDown");
  press("Enter");
  expect(isOpen()).toBe(false);
  expect(selection.get()).toMatchObject({ project: "API", ticket: "API-1" });
  // Recorded on the way: the switch, then the ticket.
  expect(ask).toHaveBeenCalledWith("palette.record", { kind: "project", project: "API" });
  expect(ask).toHaveBeenCalledWith("palette.record", { kind: "ticket", key: "API-1" });
});

test("typing > lists actions only and Backspace takes it away", async () => {
  answers([action("Park", "park")]);
  commandK();
  await settle();
  const input = host.querySelector("input") as HTMLInputElement;
  const type = (value: string) => {
    act(() => {
      // React tracks the value: set it as the browser does, under its own setter.
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  };
  type(">");
  expect(ask).toHaveBeenLastCalledWith("palette.view", expect.objectContaining({ query: ">" }));
  expect(host.querySelector(".palette-prefix")?.textContent).toBe(">");
  expect(input.value).toBe("");
  type("park");
  expect(ask).toHaveBeenLastCalledWith("palette.view", expect.objectContaining({ query: ">park" }));
  type("");
  press("Backspace");
  expect(host.querySelector(".palette-prefix")).toBeNull();
});
