import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { registered, registerScreens } from "../../app/registry";
import type { DependenciesView } from "../../generated/DependenciesView";
import { dependencies } from ".";
import { DependenciesBody, PickerBody, sendLink } from "./Dependencies";
import { highlighted, step } from "./logic";

/** SHOP-20's section (`s24-11`), as `dependencies.view` words it. */
const VIEW: DependenciesView = {
  key: "SHOP-20",
  title: "Dependencies",
  add: "Add",
  waits_for: {
    label: "Waits for",
    removable: true,
    note: "starts when both merge",
    rows: [
      {
        key: "SHOP-18",
        title: "Checkout API",
        place: "Review · your check",
        dot: "review",
      },
      {
        key: "SHOP-29",
        title: "Coupon field",
        place: "Dev · working",
        dot: "working",
      },
    ],
    empty: "SHOP-20 waits for nothing.",
  },
  waiting_for_it: {
    label: "Waiting for it",
    removable: false,
    note: null,
    rows: [{ key: "SHOP-21", title: "Order emails", place: "Backlog", dot: "queued" }],
    empty: "Nothing waits for SHOP-20.",
  },
  search: "Search by key or title",
  remove: "Remove",
  rule: "A ticket can’t wait for one that waits for it: Otaq refuses loops.",
};

describe("the Dependencies section (DPV-04)", () => {
  it("lists Waits for with its note, each row's place and ✕, and Waiting for it read-only", () => {
    const out = renderToStaticMarkup(createElement(DependenciesBody, { ticket: "SHOP-20", view: VIEW }));
    for (const text of [
      "DEPENDENCIES",
      "Add",
      "WAITS FOR · starts when both merge",
      "SHOP-18",
      "Checkout API",
      "Review · your check",
      "Dev · working",
      "WAITING FOR IT",
      "Order emails",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).toContain('aria-label="Remove SHOP-18"');
    // A ticket waiting for this one is listed, but its link is that ticket's: no ✕ here.
    expect(out).not.toContain('aria-label="Remove SHOP-21"');
  });

  it("the picker: no direction to pick, the search, its rows, then the loop rule or the refusal", () => {
    const props = {
      search: VIEW.search,
      rule: VIEW.rule,
      query: "SHOP-3",
      onQuery: () => undefined,
      rows: [{ key: "SHOP-35", title: "Slow checkout query", place: "Ready" }],
      refusal: null,
      onPick: () => undefined,
    };
    const out = renderToStaticMarkup(createElement(PickerBody, props));
    expect(out).not.toContain('role="radiogroup"');
    expect(out).not.toContain("Blocks");
    expect(out).toContain("Slow checkout query");
    expect(out).toContain(VIEW.rule);
    const refused = renderToStaticMarkup(createElement(PickerBody, { ...props, refusal: "Different bases" }));
    expect(refused).toContain('role="alert">Different bases');
  });

  it("sends the waiting ticket's link and reads a refusal's words", async () => {
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      return Promise.resolve({ outcome: "refused", message: VIEW.rule });
    };
    await expect(sendLink("SHOP-18", "SHOP-20", true, ask)).resolves.toBe(VIEW.rule);
    expect(asked).toEqual([["dependencies.link", { key: "SHOP-18", other: "SHOP-20", add: true }]]);
    const ok = () => Promise.resolve({ outcome: "linked" });
    await expect(sendLink("SHOP-20", "SHOP-29", false, ok)).resolves.toBeNull();
  });

  it("↑↓ move the highlight, stopping at the ends; the first row is lit at first", () => {
    expect(highlighted(null, 3)).toBe(0);
    expect(highlighted(5, 3)).toBe(0);
    expect(step(null, 1, 3)).toBe(1);
    expect(step(2, 1, 3)).toBe(2);
    expect(step(0, -1, 3)).toBe(0);
    expect(highlighted(null, 0)).toBeNull();
  });

  it("registers the section where Overview draws it", () => {
    expect(registered(registerScreens({ dependencies })).sheetSections).toEqual(["dependencies"]);
  });
});
