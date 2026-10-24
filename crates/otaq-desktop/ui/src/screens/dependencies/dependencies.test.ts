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
  blocked_by: {
    direction: "blocked_by",
    label: "Blocked by",
    note: "starts when both merge",
    rows: [
      {
        key: "SHOP-18",
        title: "Checkout API",
        place: "Review · your check",
        dot: "review",
        direction: "blocked_by",
      },
      {
        key: "SHOP-29",
        title: "Coupon field",
        place: "Dev · working",
        dot: "working",
        direction: "blocked_by",
      },
    ],
    empty: "SHOP-20 waits for nothing.",
  },
  blocks: { direction: "blocks", label: "Blocks", note: null, rows: [], empty: "Nothing waits for SHOP-20." },
  directions: [
    { direction: "blocked_by", label: "Blocked by" },
    { direction: "blocks", label: "Blocks" },
  ],
  search: "Search by key or title",
  remove: "Remove",
  rule: "A ticket can’t wait for one that waits for it: Otaq refuses loops.",
};

describe("the Dependencies section (DPV-04)", () => {
  it("lists Blocked by with its note and each row's place, and Blocks' empty line", () => {
    const out = renderToStaticMarkup(createElement(DependenciesBody, { ticket: "SHOP-20", view: VIEW }));
    for (const text of [
      "DEPENDENCIES",
      "Add",
      "BLOCKED BY · starts when both merge",
      "SHOP-18",
      "Checkout API",
      "Review · your check",
      "Dev · working",
      "BLOCKS",
      "Nothing waits for SHOP-20.",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).toContain('aria-label="Remove SHOP-18"');
  });

  it("the picker: the direction, the search, its rows, then the loop rule or the refusal", () => {
    const props = {
      view: VIEW,
      direction: "blocked_by" as const,
      onDirection: () => undefined,
      query: "SHOP-3",
      onQuery: () => undefined,
      rows: [{ key: "SHOP-35", title: "Slow checkout query", place: "Ready" }],
      refusal: null,
      onPick: () => undefined,
    };
    const out = renderToStaticMarkup(createElement(PickerBody, props));
    expect(out).toMatch(/aria-checked="true"[^>]*>Blocked by</);
    expect(out).toContain("Slow checkout query");
    expect(out).toContain(VIEW.rule);
    const refused = renderToStaticMarkup(createElement(PickerBody, { ...props, refusal: "Different bases" }));
    expect(refused).toContain('role="alert">Different bases');
  });

  it("sends a link with its direction and reads a refusal's words", async () => {
    const asked: [string, unknown][] = [];
    const ask = (method: string, params?: unknown) => {
      asked.push([method, params]);
      return Promise.resolve({ outcome: "refused", message: VIEW.rule });
    };
    await expect(sendLink("SHOP-18", "SHOP-20", "blocked_by", true, ask)).resolves.toBe(VIEW.rule);
    expect(asked).toEqual([
      ["dependencies.link", { key: "SHOP-18", other: "SHOP-20", direction: "blocked_by", add: true }],
    ]);
    const ok = () => Promise.resolve({ outcome: "linked" });
    await expect(sendLink("SHOP-20", "SHOP-29", "blocked_by", false, ok)).resolves.toBeNull();
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
