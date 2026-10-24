// T-0221: the header's search and filter field, the card's right-click menu and the delete confirmation.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { overlays } from "../../app/overlays";
import { registerScreens } from "../../app/registry";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import type { DeleteDialogView } from "../../generated/DeleteDialogView";
import type { HeaderView } from "../../generated/HeaderView";
import type { SearchView } from "../../generated/SearchView";
import { HeaderBar } from "../shell/Header";
import { afterMenuChoice, chooseMenuItem } from "./CardMenu";
import { DeleteBody, deleteTicket } from "./DeleteDialog";
import {
  addToken,
  boardFilter,
  clearFilter,
  filterOf,
  finishedTokens,
  focusEpic,
  isActive,
  NO_FILTER,
  removeToken,
  searchFocusRequests,
  setFilter,
} from "./filter";
import { SearchBox, stepSuggestion } from "./Search";
import { board } from ".";

afterEach(() => {
  overlays.store.set([]);
  boardFilter.set({ project: null, ...NO_FILTER });
  searchFocusRequests.set(0);
  selection.set(NOTHING_SELECTED);
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const VIEW: SearchView = {
  filter: { tokens: ["is:needs-you"], text: "checkout" },
  placeholder: "Search  ·  is:needs-you",
  keys: null,
  results: "4 results",
  suggestions_header: "FILTERS",
  suggestions: [
    { token: "is:working", description: "agent running now", count: "3" },
    { token: "dep:SHOP-18", description: "in SHOP-18’s chain", count: "3" },
  ],
  enter_adds: null,
  enter_opens_cleanup: false,
};

describe("the filter is kept per project while the app runs", () => {
  it("survives anything but switching projects", () => {
    setFilter("SHOP", { tokens: [], text: "checkout" });
    addToken("SHOP", "is:needs-you");
    expect(filterOf(boardFilter.get(), "SHOP")).toMatchObject({ tokens: ["is:needs-you"], text: "" });
    addToken("SHOP", "is:needs-you");
    addToken("SHOP", "is:working");
    removeToken("SHOP", 0);
    expect(filterOf(boardFilter.get(), "SHOP").tokens).toEqual(["is:working"]);
    expect(isActive(filterOf(boardFilter.get(), "SHOP"))).toBe(true);
    // Another project starts with no filter, and coming back doesn't bring the old one.
    expect(filterOf(boardFilter.get(), "API")).toBe(NO_FILTER);
    setFilter("API", { tokens: [], text: "x" });
    expect(filterOf(boardFilter.get(), "SHOP")).toBe(NO_FILTER);
    clearFilter("API");
    expect(isActive(filterOf(boardFilter.get(), "API"))).toBe(false);
  });

  it("takes Rust's chips only when a token finished and the user didn't type on", () => {
    const sent = { tokens: [], text: "is:needs-you " };
    const answer = { tokens: ["is:needs-you"], text: "" };
    expect(finishedTokens(sent, answer, sent)).toEqual(answer);
    expect(finishedTokens(sent, answer, { tokens: [], text: "is:needs-you c" })).toBeNull();
    const words = { tokens: [], text: "checkout " };
    expect(finishedTokens(words, words, words)).toBeNull();
  });

  it("↑/↓ walk the suggestions and wrap", () => {
    expect(stepSuggestion(3, null, "ArrowDown")).toBe(0);
    expect(stepSuggestion(3, null, "ArrowUp")).toBe(2);
    expect(stepSuggestion(3, 2, "ArrowDown")).toBe(0);
    expect(stepSuggestion(3, 0, "ArrowUp")).toBe(2);
    expect(stepSuggestion(0, null, "ArrowDown")).toBeNull();
  });
});

describe("the search field", () => {
  const box = (over: Partial<Parameters<typeof SearchBox>[0]> = {}) =>
    renderToStaticMarkup(
      createElement(SearchBox, {
        filter: VIEW.filter,
        view: VIEW,
        focused: false,
        highlighted: null,
        onType: () => undefined,
        onRemove: () => undefined,
        onPick: () => undefined,
        onKeyDown: () => undefined,
        onFocusChange: () => undefined,
        ...over,
      }),
    );

  it("draws the chips with their ×, what is typed and the count, as *Search & filter*", () => {
    const html = box();
    expect(html).toContain('data-tauri-drag-region="false"');
    expect(html).toContain("is:needs-you");
    expect(html).toContain('aria-label="Remove is:needs-you"');
    expect(html).toContain('value="checkout"');
    expect(html).toContain("4 results");
    expect(html).not.toContain('placeholder="Search');
  });

  it("shows the placeholder and the filter key while no filter is active", () => {
    const html = box({
      filter: NO_FILTER,
      view: { ...VIEW, filter: NO_FILTER, results: null, keys: "/" },
    });
    expect(html).toContain('placeholder="Search  ·  is:needs-you"');
    expect(html).toContain(">/<");
    expect(html).not.toContain("results");
  });

  it("replaces the header's search entry", () => {
    const header = {
      project: { key: "SHOP", name: "shop-web", base_branch: "main", path: "~/shop-web", flow_setup: null },
    };
    const view = {
      ...header,
      search: { text: "Search  ·  is:needs-you", keys: "/" },
      activity: {
        agents: { count: "0", label: "running", active: false, control: false },
        processes: null,
        warning: null,
        hover: { title: "0 agents running across all projects", lines: [] },
        label: "Activity",
        keys: null,
      },
    } as unknown as HeaderView;
    const html = renderToStaticMarkup(
      createElement(HeaderBar, {
        view,
        onSearch: () => undefined,
        search: createElement("div", { className: "the-field" }),
      }),
    );
    expect(html).toContain("the-field");
    expect(html).not.toContain("header-search");
  });

  it("Filter the board asks the field for the keyboard while a project is shown", () => {
    const app = registerScreens({ board });
    expect(app.shortcuts.run({ action: "filter_board" })).toBe(false);
    selection.set({ ...NOTHING_SELECTED, project: "SHOP" });
    expect(app.shortcuts.run({ action: "filter_board" })).toBe(true);
    expect(searchFocusRequests.get()).toBe(1);
  });
});

describe("the card menu", () => {
  it("opens each dialog by its overlay id", () => {
    afterMenuChoice({ kind: "sent" }, "SHOP-21");
    expect(overlays.store.get()).toEqual([]);
    afterMenuChoice({ kind: "delete" }, "SHOP-21");
    afterMenuChoice({ kind: "close_ticket" }, "SHOP-19");
    afterMenuChoice({ kind: "move_to_epic" }, "SHOP-51");
    afterMenuChoice({ kind: "close_epic" }, "SHOP-40");
    expect(overlays.store.get()).toEqual([
      { id: "delete_ticket", params: { ticket: "SHOP-21" } },
      { id: "close_ticket", params: { ticket: "SHOP-19" } },
      { id: "move_to_epic", params: { ticket: "SHOP-51" } },
      { id: "close_epic", params: { epic: "SHOP-40" } },
    ]);
  });

  it("does what the board does for an epic's own items (T-0274)", () => {
    const done: unknown[] = [];
    const board = {
      drop: (key: string, column: string) => done.push(["drop", key, column]),
      focus: (key: string) => done.push(["focus", key]),
    };
    afterMenuChoice({ kind: "queue_epic" }, "SHOP-40", board);
    afterMenuChoice({ kind: "take_back_epic" }, "SHOP-40", board);
    afterMenuChoice({ kind: "focus_epic" }, "SHOP-40", board);
    afterMenuChoice({ kind: "add_to_epic" }, "SHOP-40", board);
    afterMenuChoice({ kind: "open_epic" }, "SHOP-40", board);
    expect(done).toEqual([
      ["drop", "SHOP-40", "ready_for_dev"],
      ["drop", "SHOP-40", "backlog"],
      ["focus", "SHOP-40"],
    ]);
    expect(overlays.store.get()).toEqual([{ id: "new_ticket", params: { epic: "SHOP-40" } }]);
    expect(selection.get().ticket).toBe("SHOP-40");
  });

  it("sends the chosen item to Rust and reports a refusal", async () => {
    const calls: unknown[] = [];
    await chooseMenuItem(
      "SHOP-21",
      "pin",
      () => undefined,
      (method, params) => {
        calls.push([method, params]);
        return Promise.resolve({ kind: "delete" });
      },
    );
    expect(calls).toEqual([["board.menu_choose", { key: "SHOP-21", id: "pin" }]]);
    expect(overlays.store.get()).toEqual([{ id: "delete_ticket", params: { ticket: "SHOP-21" } }]);
    const errors: unknown[] = [];
    await chooseMenuItem(
      "SHOP-21",
      "priority-1",
      (e) => errors.push(e),
      () => Promise.reject(new Error("no")),
    );
    expect(errors).toHaveLength(1);
  });
});

describe("focusing an epic (EPC-18)", () => {
  it("takes Rust's filter with the epic's token, and leaves it alone for a standalone ticket", async () => {
    setFilter("SHOP", { tokens: ["is:working"], text: "wish" });
    const calls: unknown[] = [];
    const changed = await focusEpic("SHOP", "SHOP-40.2", (method, params) => {
      calls.push([method, params]);
      return Promise.resolve({ tokens: ["is:working", "epic:Wishlist"], text: "wish" });
    });
    expect(changed).toBe(true);
    expect(calls).toEqual([
      [
        "board.focus",
        { project: "SHOP", filter: { tokens: ["is:working"], text: "wish" }, key: "SHOP-40.2" },
      ],
    ]);
    expect(filterOf(boardFilter.get(), "SHOP").tokens).toEqual(["is:working", "epic:Wishlist"]);
    const nothing = await focusEpic("SHOP", "SHOP-51", () => Promise.resolve(null));
    expect(nothing).toBe(false);
    expect(filterOf(boardFilter.get(), "SHOP").tokens).toEqual(["is:working", "epic:Wishlist"]);
  });
});

describe("the delete confirmation", () => {
  const VIEW_DELETE: DeleteDialogView = {
    title: "Delete SHOP-21 Order confirmation email?",
    body: [
      "It never started, so nothing on disk is affected.",
      "SHOP-22 depends on it; that link will be removed.",
    ],
    confirm: "Delete ticket",
    confirm_icon: "trash-2",
    error: null,
  };

  it("says it as one paragraph, with a refusal under it", () => {
    const html = renderToStaticMarkup(createElement(DeleteBody, { view: VIEW_DELETE, error: null }));
    expect(html).toContain(
      "It never started, so nothing on disk is affected. SHOP-22 depends on it; that link will be removed.",
    );
    expect(html).not.toContain('role="alert"');
    const refused = renderToStaticMarkup(
      createElement(DeleteBody, {
        view: { ...VIEW_DELETE, body: [], error: "SHOP-19 has started; close it instead" },
        error: null,
      }),
    );
    expect(refused).toContain("SHOP-19 has started; close it instead");
  });

  it("deletes and closes the sheet that showed the ticket", async () => {
    selection.set({ ...NOTHING_SELECTED, project: "SHOP", ticket: "SHOP-21" });
    const calls: unknown[] = [];
    await deleteTicket("SHOP-21", (method, params) => {
      calls.push([method, params]);
      return Promise.resolve(null);
    });
    await flush();
    expect(calls).toEqual([["board.delete", { key: "SHOP-21" }]]);
    expect(selection.get().ticket).toBeNull();
  });
});
