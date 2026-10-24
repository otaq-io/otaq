// @vitest-environment happy-dom
// The List view's own rules (T-0461; LST-01, LST-02): it draws Rust's header, groups and rows as given; ↑/↓ walk the
// rows; only Group by Stage's groups take a drop, measured from head to last row; Start sends the drop it carries; a
// row's buttons don't start the row's press; Done's "show" sends its fold.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ListGroup } from "../../generated/ListGroup";
import type { ListRow } from "../../generated/ListRow";
import type { ListView } from "../../generated/ListView";
import { List, type ListHandlers } from "./List";
import { dragsRows, findRow, groupSpans, listTargets, nextRow, rowKeys } from "./rows";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function row(
  key: string,
  title: string,
  extra: Partial<ListRow> = {},
  card: Partial<ListRow["card"]> = {},
): ListRow {
  return {
    card: {
      key,
      key_label: key,
      epic: null,
      priority: null,
      title,
      status: null,
      blocks: null,
      avatar: null,
      who: null,
      time: null,
      edge: null,
      action: null,
      origin: null,
      hover: { status: "", details: [] },
      faded: null,
      fresh: false,
      open_panel: "Open panel",
      open_panel_keys: "↵",
      open_terminal: "Open terminal",
      ...card,
    },
    icon: "circle-dashed",
    palette: "queued",
    track: {
      steps: [
        { name: "Grill", fill: null },
        { name: "Dev", fill: null },
      ],
      label: "—",
    },
    start: null,
    needs_you: false,
    dim: false,
    ...extra,
  };
}

function group(id: string, title: string, rows: ListRow[], extra: Partial<ListGroup> = {}): ListGroup {
  return {
    id,
    title,
    icon: "circle-dashed",
    tone: null,
    count: String(rows.length),
    hint: null,
    toggle: null,
    folded: false,
    empty: rows.length === 0 ? "Nothing here" : null,
    target: null,
    entries: rows.map((r) => ({ kind: "ticket", row: r })),
    ...extra,
  };
}

/** `list_view.rs`'s words for a small shop-web, by stage. */
const LIST: ListView = {
  header: { ticket: "TICKET", flow: "WHERE IN THE FLOW", happening: "WHAT’S HAPPENING", who: "WHO" },
  groups: [
    group(
      "backlog",
      "Backlog",
      [row("SHOP-41", "Fix README typo", { start: { label: "Start", target: "grill" } })],
      {
        hint: "drag a row to Ready, or Start",
        target: "backlog",
      },
    ),
    group("ready_for_dev", "Ready", [row("SHOP-35", "Slow checkout query")], {
      hint: "starts in priority order",
      target: "ready_for_dev",
    }),
    group(
      "grill",
      "Grill",
      [
        row(
          "SHOP-12",
          "Add dark mode toggle",
          {
            icon: "message-circle-question-mark",
            palette: "attn",
            needs_you: true,
            track: {
              steps: [
                { name: "Grill", fill: "attn" },
                { name: "Dev", fill: null },
              ],
              label: "Grill",
            },
          },
          {
            status: {
              text: "Asks you: follow the system setting?",
              dot: "attn",
              tone: "attention",
              icon: null,
            },
            edge: "attention",
            who: "Griller",
            time: { text: "8m", stale: false },
            action: { kind: "open", label: "Reply", icon: null, primary: true },
          },
        ),
      ],
      { target: "grill" },
    ),
    group("validate", "Validate", [], { target: null }),
    group("done", "Done", [], {
      count: "4",
      hint: "4 this week · show",
      toggle: { kind: "fold_column", column: "done", folded: false },
      folded: true,
      empty: null,
      target: "done",
    }),
  ],
};

function handlers() {
  return {
    press: vi.fn<ListHandlers["press"]>(),
    key: vi.fn<ListHandlers["key"]>(),
    action: vi.fn<ListHandlers["action"]>(),
    start: vi.fn<ListHandlers["start"]>(),
    menu: vi.fn<ListHandlers["menu"]>(),
    open: vi.fn<ListHandlers["open"]>(),
  };
}

describe("the List's rows", () => {
  it("walks the rows top to bottom and finds a row's group", () => {
    expect(rowKeys(LIST)).toEqual(["SHOP-41", "SHOP-35", "SHOP-12"]);
    expect(nextRow(rowKeys(LIST), "SHOP-35", 1)).toBe("SHOP-12");
    expect(nextRow(rowKeys(LIST), "SHOP-41", -1)).toBeNull();
    expect(nextRow(rowKeys(LIST), "SHOP-99", 1)).toBeNull();
    expect(findRow(LIST, "SHOP-12")?.group.title).toBe("Grill");
    expect(findRow(LIST, "SHOP-99")).toBeNull();
  });

  it("takes drops only on the groups with a target, from head to last row", () => {
    expect(listTargets(LIST)).toEqual(["backlog", "ready_for_dev", "grill", "done"]);
    expect(dragsRows(LIST)).toBe(true);
    const byStatus: ListView = { ...LIST, groups: LIST.groups.map((g) => ({ ...g, target: null })) };
    expect(dragsRows(byStatus)).toBe(false);
    const el = {
      getBoundingClientRect: () => ({ left: 80, right: 1200, top: 300, bottom: 420 }),
    } as HTMLElement;
    expect(groupSpans(new Map([["ready_for_dev", el]]))).toEqual([
      { id: "ready_for_dev", left: 80, right: 1200, top: 300, bottom: 420 },
    ]);
  });
});

describe("the List", () => {
  it("draws the header, the groups' heads and the rows' words", () => {
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
      "TICKET",
      "WHERE IN THE FLOW",
      "WHAT’S HAPPENING",
      "WHO",
      "drag a row to Ready, or Start",
      "starts in priority order",
      "Asks you: follow the system setting?",
      "Griller",
      "8m",
      "Reply",
      "Start",
      "Nothing here",
      "4 this week · show",
    ]) {
      expect(html).toContain(text);
    }
    expect(html).toContain('data-row="SHOP-12" data-selected="true"');
    expect(html).toContain('data-edge="attention"');
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

    it("sends Start's drop, the action, Done's fold and the row's press", () => {
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
              t === "ready_for_dev"
                ? { kind: "allowed", icon: "plus", hint: "Queue", note: null }
                : undefined,
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
      expect(h.start).toHaveBeenCalledWith("SHOP-41", "grill");
      expect(h.press).not.toHaveBeenCalled();
      act(() => {
        button("Reply").click();
      });
      expect(h.action).toHaveBeenCalledWith("SHOP-12", "open");
      act(() => {
        button("4 this week · show").click();
      });
      expect(onView).toHaveBeenCalledWith({ kind: "fold_column", column: "done", folded: false });
      const shop35 = host.querySelector('[data-row="SHOP-35"]') as HTMLElement;
      act(() => {
        shop35.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
      });
      expect(h.press).toHaveBeenCalledWith("SHOP-35", expect.anything());
      // Group by Stage's targets registered their elements; Validate (no target) didn't.
      expect([...groups.keys()].sort()).toEqual(["backlog", "done", "grill", "ready_for_dev"]);
      expect(host.querySelector('[aria-label="Ready"]')?.getAttribute("data-verdict")).toBe("allowed");
    });

    it("draws an epic's parent row with its children; a click opens it, the chevron folds it", () => {
      const h = handlers();
      const onView = vi.fn();
      const child = row("SHOP-40.3", "Wishlist page");
      const byEpic: ListView = {
        ...LIST,
        groups: [
          {
            ...group("epics", "Epics", []),
            count: "1",
            empty: null,
            entries: [
              {
                kind: "parent",
                parent: {
                  row: {
                    key: "SHOP-40",
                    external_id: "PAY-123",
                    name: "Wishlist",
                    track: { stages: [], at: 0, label: "Dev" },
                    line: "1 of 5 merged · Wishlist lead is planning",
                    lead: null,
                    needs_you: true,
                  },
                  track: { steps: [{ name: "Dev", fill: "working" }], label: "Dev" },
                  folded: false,
                  fold: { kind: "fold_lane", epic: "SHOP-40", folded: true },
                  fold_label: "Fold",
                  children: [child],
                },
              },
            ],
          },
        ],
      };
      expect(rowKeys(byEpic)).toEqual(["SHOP-40.3"]);
      act(() => {
        root.render(
          createElement(List, {
            list: byEpic,
            selected: null,
            lifted: null,
            verdict: () => undefined,
            handlers: h,
            group: () => undefined,
            onView,
          }),
        );
      });
      expect(host.textContent).toContain("PAY-123");
      expect(host.textContent).toContain("1 of 5 merged · Wishlist lead is planning");
      expect(host.querySelector('[data-row="SHOP-40.3"]')).not.toBeNull();
      act(() => {
        (host.querySelector(".list-parent-fold") as HTMLButtonElement).click();
      });
      expect(onView).toHaveBeenCalledWith({ kind: "fold_lane", epic: "SHOP-40", folded: true });
      expect(h.open).not.toHaveBeenCalled();
      act(() => {
        (host.querySelector(".list-parent") as HTMLElement).click();
      });
      expect(h.open).toHaveBeenCalledWith("SHOP-40");
    });
  });
});
