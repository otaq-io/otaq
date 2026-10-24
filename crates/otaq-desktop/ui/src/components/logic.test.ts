import { describe, expect, it } from "vitest";

import type { MenuItem } from "../generated/MenuItem";
import {
  acceptsPress,
  chooseMenu,
  codeSpans,
  defaultOutcome,
  dialogKey,
  EDGE,
  filterMenu,
  firstMatch,
  FLOAT_GAP,
  MARK_COLUMNS,
  MARK_FLAG_OVERLAP,
  MARK_MIN_STROKE,
  MARK_OUTLINE,
  MARK_VIEWBOX,
  markStrokes,
  menuKey,
  placeFloating,
  searchKey,
  stepMenu,
  trapFocus,
} from "./logic";
import { arrowIndex } from "./Tabs";

const item = (id: string, change: Partial<MenuItem> = {}): MenuItem => ({
  id,
  label: id,
  icon: null,
  shortcut: null,
  danger: false,
  disabled: false,
  hint: null,
  separated: false,
  checked: false,
  header: false,
  ...change,
});

// The same menu as `menu.rs`'s tests, so both sides keep one rule.
const menu = [
  item("open", { shortcut: "↵" }),
  item("pin", { separated: true }),
  item("close", { danger: true }),
  item("delete", { danger: true, disabled: true, hint: "Started tickets can only be closed." }),
];

describe("menu keyboard (menu.rs, MenuModel)", () => {
  it("arrows skip disabled rows and wrap; up from nothing starts at the bottom", () => {
    let at = stepMenu(menu, null, 1);
    expect(at).toBe(0);
    at = stepMenu(menu, stepMenu(menu, at, 1), 1);
    expect(at).toBe(2);
    expect(stepMenu(menu, at, 1)).toBe(0);
    expect(stepMenu(menu, 0, -1)).toBe(2);
    expect(stepMenu(menu, null, -1)).toBe(2);
  });

  it("↵ and clicks choose enabled rows only", () => {
    expect(chooseMenu(menu, null)).toBeNull();
    expect(chooseMenu(menu, 0)).toBe("open");
    expect(chooseMenu(menu, 1)).toBe("pin");
    expect(chooseMenu(menu, 3)).toBeNull();
  });

  it("skips headers; an empty or all-disabled menu highlights nothing", () => {
    const list = [
      item("", { header: true, label: "LOCAL" }),
      item("main"),
      item("", { header: true }),
      item("next"),
    ];
    expect(stepMenu(list, null, 1)).toBe(1);
    expect(stepMenu(list, 1, 1)).toBe(3);
    expect(chooseMenu(list, 0)).toBeNull();
    expect(stepMenu([item("x", { disabled: true })], null, 1)).toBeNull();
    expect(stepMenu([], null, -1)).toBeNull();
  });

  it("maps keys: arrows, Home/End, ↵ and Space choose, Esc and Tab close", () => {
    expect(menuKey(menu, null, "ArrowDown")).toEqual({ kind: "highlight", index: 0 });
    expect(menuKey(menu, 0, "ArrowUp")).toEqual({ kind: "highlight", index: 2 });
    expect(menuKey(menu, 2, "Home")).toEqual({ kind: "highlight", index: 0 });
    expect(menuKey(menu, 0, "End")).toEqual({ kind: "highlight", index: 2 });
    expect(menuKey(menu, 2, "Enter")).toEqual({ kind: "choose", id: "close" });
    expect(menuKey(menu, 1, " ")).toEqual({ kind: "choose", id: "pin" });
    expect(menuKey(menu, null, "Enter")).toEqual({ kind: "none" });
    expect(menuKey(menu, 0, "Escape")).toEqual({ kind: "close" });
    expect(menuKey(menu, 0, "Tab")).toEqual({ kind: "close" });
    expect(menuKey(menu, 0, "a")).toEqual({ kind: "none" });
  });
});

describe("searchable lists", () => {
  const branches = [
    item("", { header: true, label: "LOCAL" }),
    item("main", { checked: true }),
    item("release/2.3"),
    item("", { header: true, label: "REMOTE" }),
    item("origin/release/2.2", { label: "release/2.2" }),
    item("origin/next", { label: "next" }),
  ];
  const ids = (list: readonly MenuItem[]) => list.map((i) => (i.header ? `#${i.label}` : i.id));

  it("keeps the rows matching the label or id, any case, with their headers", () => {
    expect(filterMenu(branches, "")).toBe(branches);
    expect(ids(filterMenu(branches, "RELEASE"))).toEqual([
      "#LOCAL",
      "release/2.3",
      "#REMOTE",
      "origin/release/2.2",
    ]);
    expect(ids(filterMenu(branches, " origin/ "))).toEqual(["#REMOTE", "origin/release/2.2", "origin/next"]);
    expect(filterMenu(branches, "nothing")).toEqual([]);
  });

  it("highlights the checked row, else the first match", () => {
    expect(firstMatch(branches)).toBe(1);
    expect(firstMatch(filterMenu(branches, "next"))).toBe(1);
    expect(firstMatch([])).toBeNull();
  });

  it("leaves Space, Home and End to the search field", () => {
    expect(searchKey(branches, 1, " ")).toEqual({ kind: "none" });
    expect(searchKey(branches, 1, "Home")).toEqual({ kind: "none" });
    expect(searchKey(branches, 1, "End")).toEqual({ kind: "none" });
    expect(searchKey(branches, 1, "ArrowDown")).toEqual({ kind: "highlight", index: 2 });
    expect(searchKey(branches, 2, "Enter")).toEqual({ kind: "choose", id: "release/2.3" });
    expect(searchKey(branches, 2, "Escape")).toEqual({ kind: "close" });
  });
});

describe("dialogs (dialog.rs)", () => {
  it("Esc cancels; ↵ presses the default: confirm, but Cancel on a destructive dialog", () => {
    expect(dialogKey(false, "Escape")).toBe("cancel");
    expect(dialogKey(true, "Escape")).toBe("cancel");
    expect(dialogKey(false, "Enter")).toBe("confirm");
    expect(dialogKey(true, "Enter")).toBe("cancel");
    expect(dialogKey(false, "a")).toBeNull();
    expect(defaultOutcome(true)).toBe("cancel");
    expect(defaultOutcome(false)).toBe("confirm");
  });

  it("Tab and ⇧Tab cycle inside the dialog", () => {
    expect(trapFocus(3, 0, false)).toBe(1);
    expect(trapFocus(3, 2, false)).toBe(0);
    expect(trapFocus(3, 0, true)).toBe(2);
    expect(trapFocus(3, -1, false)).toBe(0);
    expect(trapFocus(3, -1, true)).toBe(2);
    expect(trapFocus(0, -1, false)).toBe(-1);
  });
});

describe("floating layers", () => {
  const viewport = { width: 1280, height: 800 };
  const size = { width: 260, height: 200 };

  it("go under the anchor, left edges aligned, or right edges with `end`", () => {
    const anchor = { left: 100, top: 100, width: 80, height: 28 };
    expect(placeFloating(anchor, size, viewport)).toEqual({ left: 100, top: 128 + FLOAT_GAP });
    expect(placeFloating(anchor, size, viewport, "end")).toEqual({ left: EDGE, top: 128 + FLOAT_GAP });
  });

  it("flip above when there is no room below, and stay inside the window", () => {
    const low = { left: 1200, top: 700, width: 60, height: 28 };
    expect(placeFloating(low, size, viewport)).toEqual({
      left: 1280 - EDGE - 260,
      top: 700 - FLOAT_GAP - 200,
    });
    const tall = { width: 260, height: 900 };
    expect(placeFloating({ left: 0, top: 10, width: 10, height: 10 }, tall, viewport).top).toBe(EDGE);
  });
});

describe("buttons and tabs", () => {
  it("a press reaches the action only when neither disabled nor loading", () => {
    expect(acceptsPress(false, false)).toBe(true);
    expect(acceptsPress(true, false)).toBe(false);
    expect(acceptsPress(false, true)).toBe(false);
  });

  it("←/→ move between tabs and segments, wrapping", () => {
    expect(arrowIndex("ArrowRight", 5, 6)).toBe(0);
    expect(arrowIndex("ArrowLeft", 0, 6)).toBe(5);
    expect(arrowIndex("ArrowDown", 0, 6)).toBeNull();
    expect(arrowIndex("ArrowRight", 0, 0)).toBeNull();
  });
});

describe("markStrokes", () => {
  const onScreen = (units: number, size: number) => (units * size) / MARK_VIEWBOX;

  it("keeps the design's widths at the About size", () => {
    expect(markStrokes(64)).toMatchObject({ outline: MARK_OUTLINE, columns: MARK_COLUMNS });
  });

  it("starts the flag at the pole's edge", () => {
    // The design moves the flag 15/7 units right of the pole's centre at full size.
    expect(markStrokes(64).flagShift).toBeCloseTo(15 / 7);
    const { columns, flagShift } = markStrokes(16);
    expect(flagShift).toBeCloseTo(columns / 2 - MARK_FLAG_OVERLAP);
  });

  it("never draws a stroke thinner than the minimum at small sizes", () => {
    for (const size of [12, 16, 20, 24]) {
      const { outline, columns } = markStrokes(size);
      expect(onScreen(outline, size)).toBeGreaterThanOrEqual(MARK_MIN_STROKE - 1e-9);
      expect(onScreen(columns, size)).toBeGreaterThanOrEqual(MARK_MIN_STROKE - 1e-9);
    }
  });

  it("thickens only what would drop below the minimum", () => {
    // At 24 px the outline (1.3 px) is fine and only the columns (0.92 px) thicken.
    const { outline, columns } = markStrokes(24);
    expect(outline).toBe(MARK_OUTLINE);
    expect(onScreen(columns, 24)).toBeCloseTo(MARK_MIN_STROKE);
  });
});

describe("codeSpans", () => {
  it("draws the backticked parts as code", () => {
    expect(codeSpans("For example `npm test`.")).toEqual([
      { text: "For example ", code: false },
      { text: "npm test", code: true },
      { text: ".", code: false },
    ]);
    expect(codeSpans("Runs `a` then `b`")).toEqual([
      { text: "Runs ", code: false },
      { text: "a", code: true },
      { text: " then ", code: false },
      { text: "b", code: true },
    ]);
    expect(codeSpans("no code")).toEqual([{ text: "no code", code: false }]);
    expect(codeSpans("one ` tick")).toEqual([{ text: "one ` tick", code: false }]);
  });
});
