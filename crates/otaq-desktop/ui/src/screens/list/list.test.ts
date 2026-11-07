// @vitest-environment happy-dom
// The List layout's own rules: it draws Rust's groups and rows as given (the card's words, the edge only where Rust
// gives one); ↑/↓ walk the rows; every group and the Done line take a drop, measured from head to last row; a row's
// button doesn't start the row's press; a head's chevron and the Done line send their fold.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ListGroup } from "../../generated/ListGroup";
import type { ListRow } from "../../generated/ListRow";
import type { ListView } from "../../generated/ListView";
import { List, type ListHandlers } from "./List";
import { findRow, groupSpans, listTargets, nextRow, rowKeys } from "./rows";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function row(key: string, title: string, card: Partial<ListRow["card"]> = {}): ListRow {
  return {
    card: {
      key,
      priority: null,
      title,
      status: null,
      blocks: null,
      avatar: null,
      who: null,
      time: null,
      edge: null,
      action: null,
      note: null,
      opens: null,
      hover: { status: "", details: [] },
      faded: null,
      fresh: false,
      open_sheet: "Open panel",
      open_sheet_keys: "↵",
      open_terminal: "Open terminal",
      ...card,
    },
  };
}

function group(id: string, title: string, rows: ListRow[], extra: Partial<ListGroup> = {}): ListGroup {
  return {
    id,
    title,
    count: String(rows.length),
    folded: false,
    toggle: { kind: "fold_column", column: id, folded: true },
    empty: rows.length === 0 ? "Nothing here" : null,
    target: id,
    entries: rows.map((r) => ({ kind: "ticket", row: r })),
    ...extra,
  };
}

/** `list_view.rs`'s words for a small shop-web: the stages, then Backlog, then Done folded. */
const LIST: ListView = {
  groups: [
    group("product-spec", "Product spec", [
      row("SHOP-12", "Add dark mode toggle", {
        status: {
          text: "Asks you · follow the system setting?",
          dot: "attn",
          tone: "attention",
          icon: null,
        },
        edge: "attention",
        who: "Spec writer",
        time: { text: "8m", stale: false },
        action: { kind: "open", label: "Answer", icon: null, primary: false },
      }),
    ]),
    group("dev", "Dev", [
      row("SHOP-35", "Slow checkout query", {
        status: { text: "Working", dot: "working", tone: "secondary", icon: null },
      }),
    ]),
    group("validate", "Validate", [], { folded: true, toggle: null, empty: null }),
    group("backlog", "Backlog", [
      row("SHOP-41", "Fix README typo", {
        status: { text: "Not started", dot: "queued", tone: "secondary", icon: null },
        action: { kind: "start", label: "Start", icon: null, primary: false },
      }),
    ]),
  ],
  done: {
    text: "Done · 4",
    unfold: { kind: "fold_column", column: "done", folded: false },
    target: "done",
  },
};

function handlers() {
  return {
    press: vi.fn<ListHandlers["press"]>(),
    key: vi.fn<ListHandlers["key"]>(),
    action: vi.fn<ListHandlers["action"]>(),
    menu: vi.fn<ListHandlers["menu"]>(),
  };
}

describe("the List's rows", () => {
  it("walks the rows top to bottom and finds a row's group", () => {
    expect(rowKeys(LIST)).toEqual(["SHOP-12", "SHOP-35", "SHOP-41"]);
    expect(nextRow(rowKeys(LIST), "SHOP-35", 1)).toBe("SHOP-41");
    expect(nextRow(rowKeys(LIST), "SHOP-12", -1)).toBeNull();
    expect(nextRow(rowKeys(LIST), "SHOP-99", 1)).toBeNull();
    expect(findRow(LIST, "SHOP-12")?.group.title).toBe("Product spec");
    expect(findRow(LIST, "SHOP-99")).toBeNull();
  });

  it("takes drops on every group and the Done line, from head to last row", () => {
    expect(listTargets(LIST)).toEqual(["product-spec", "dev", "validate", "backlog", "done"]);
    expect(listTargets({ ...LIST, done: null })).toEqual(["product-spec", "dev", "validate", "backlog"]);
    const el = {
      getBoundingClientRect: () => ({ left: 80, right: 1200, top: 300, bottom: 420 }),
    } as HTMLElement;
    expect(groupSpans(new Map([["dev", el]]))).toEqual([
      { id: "dev", left: 80, right: 1200, top: 300, bottom: 420 },
    ]);
  });
});

describe("the List", () => {
  it("draws the groups' heads and the rows in the card's words, with no header", () => {
    const html = renderToStaticMarkup(
      createElement(List, {
        list: LIST,
        selected: "SHOP-12",
        lifted: null,
        verdict: () => undefined,
        handlers: handlers(),
        group: () => undefined,
        onView: () => undefined,
      }),
    );
    for (const text of [
      "Product spec",
      "Asks you · follow the system setting?",
      "Spec writer",
      "8m",
      "Answer",
      "Start",
      "Not started",
      "Done · 4",
    ]) {
      expect(html).toContain(text);
    }
    expect(html).not.toContain("WHERE IN THE FLOW");
    expect(html).toContain('data-row="SHOP-12" data-selected="true"');
    // Only what needs you has the edge.
    expect(html.match(/data-edge=/g)?.length).toBe(1);
    expect(html).toContain('data-edge="attention"');
    // A folded group is its head alone.
    expect(html).toMatch(/aria-label="Validate" data-folded="true"/);
    // The selected row is the one tab stop.
    expect(html.match(/tabindex="0"/g)?.length).toBe(1);
  });

  describe("in the page", () => {
    let host: HTMLDivElement;
    let root: Root;
    beforeEach(() => {
      host = document.createElement("div");
      document.body.append(host);
      root = createRoot(host);
    });
    afterEach(() => {
      act(() => {
        root.unmount();
      });
      host.remove();
    });

    it("sends the action, the folds and the row's press, and registers every drop target", () => {
      const h = handlers();
      const onView = vi.fn();
      const groups = new Map<string, HTMLElement | null>();
      act(() => {
        root.render(
          createElement(List, {
            list: LIST,
            selected: null,
            lifted: null,
            verdict: (t) =>
              t === "dev" ? { kind: "allowed", icon: "plus", hint: "Start", note: null } : undefined,
            handlers: h,
            group: (id, el) => groups.set(id, el),
            onView,
          }),
        );
      });
      const button = (label: string) =>
        [...host.querySelectorAll("button")].find((b) => b.textContent === label) as HTMLButtonElement;
      act(() => {
        button("Start").dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
        button("Start").click();
      });
      expect(h.action).toHaveBeenCalledWith("SHOP-41", "start");
      expect(h.press).not.toHaveBeenCalled();
      act(() => {
        button("Done · 4").click();
      });
      expect(onView).toHaveBeenCalledWith({ kind: "fold_column", column: "done", folded: false });
      act(() => {
        button("Dev1").click();
      });
      expect(onView).toHaveBeenCalledWith({ kind: "fold_column", column: "dev", folded: true });
      const shop35 = host.querySelector('[data-row="SHOP-35"]') as HTMLElement;
      act(() => {
        shop35.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
      });
      expect(h.press).toHaveBeenCalledWith("SHOP-35", expect.anything());
      expect([...groups.keys()].sort()).toEqual(["backlog", "dev", "done", "product-spec", "validate"]);
      expect(host.querySelector('[aria-label="Dev"]')?.getAttribute("data-verdict")).toBe("allowed");
    });
  });
});
