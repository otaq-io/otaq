import { describe, expect, it } from "vitest";

import { overlays } from "../../app/overlays";
import { openReport } from "./reportOpener";
import { registerReportOpener } from "./ReportView";
import {
  exact,
  firstLine,
  listHeight,
  MAX_HEIGHT,
  pagesFor,
  ROW_HEIGHT,
  scrollFor,
  visibleRows,
} from "./reportLogic";

describe("the report view's scrolling (CMT-12)", () => {
  it("shows the lines under the scroll position, exactly while the list fits", () => {
    // s05-41: 2,310 lines, line 1,180 at the top.
    expect(exact(2_310)).toBe(true);
    expect(listHeight(2_310)).toBe(2_310 * ROW_HEIGHT);
    expect(firstLine(1_179 * ROW_HEIGHT + 5, 400, 2_310)).toBe(1_179);
    expect(visibleRows(400)).toBe(21);
    expect(scrollFor(1_190, 300, 2_310)).toBe(1_190 * ROW_HEIGHT - 100);
    expect(scrollFor(0, 300, 2_310)).toBe(0);
  });

  it("asks only for the pages it shows", () => {
    expect(pagesFor(1_179, 21, 200, 2_310)).toEqual([5, 6]);
    expect(pagesFor(0, 21, 200, 2_310)).toEqual([0]);
    expect(pagesFor(2_300, 21, 200, 2_310)).toEqual([11]);
    expect(pagesFor(0, 21, 200, 0)).toEqual([]);
  });

  it("maps a report taller than the browser draws onto its lines", () => {
    const total = 10_000_000;
    expect(exact(total)).toBe(false);
    expect(listHeight(total)).toBe(MAX_HEIGHT);
    expect(firstLine(0, 400, total)).toBe(0);
    const last = total - visibleRows(400) + 1;
    expect(firstLine(MAX_HEIGHT - 400, 400, total)).toBe(last);
    // A match far down scrolls to its line.
    const line = 7_500_000;
    expect(Math.abs(firstLine(scrollFor(line, 400, total), 400, total) - line)).toBeLessThanOrEqual(1);
  });
});

describe("Open on a report chip (CMT-12)", () => {
  it("opens the report view over the sheet with what to read", () => {
    registerReportOpener();
    overlays.store.set([]);
    openReport({ kind: "comment", ticket: "SHOP-34", n: 4 });
    expect(overlays.store.get()).toEqual([
      { id: "report", params: { kind: "comment", ticket: "SHOP-34", n: 4 } },
    ]);
    openReport({ kind: "pasted", ticket: "SHOP-34", text: "log", size: "· 3 bytes" });
    expect(overlays.store.get()).toEqual([
      { id: "report", params: { kind: "pasted", ticket: "SHOP-34", text: "log" } },
    ]);
    overlays.store.set([]);
  });
});
