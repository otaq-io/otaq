// @vitest-environment happy-dom
// The epic's sheet (SHT-07, T-0469; `s25-11`): its header (no Move…, "1 of 5 merged", the lead's state, Tickets with
// its count), what the lead asks as buttons (a click sends that option), the TICKETS grid, the Tickets tab, and the
// tab registered from its folder.
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { registered, registerScreens } from "../../app/registry";
import type { EpicGrid as Grid } from "../../generated/EpicGrid";
import type { EpicTicketsView } from "../../generated/EpicTicketsView";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import type { SheetView } from "../../generated/SheetView";
import { request } from "../../tauri";
import { renderSheet, SHEET, VIEW } from "../sheet/sheet_fixtures";
import { epic } from ".";
import { EpicGrid, LeadAsks, TicketsBody } from "./EpicSheet";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  vi.mocked(request).mockClear();
  document.body.innerHTML = "";
});

const ASKS: SheetAnswer = {
  asker: "Wishlist lead asks",
  question: "q-lead",
  text: "40.4 Share link conflicts with 40.2 in WishlistButton.tsx. I’ll start it after 40.2 merges unless you say otherwise.",
  rows: [
    { number: 1, label: "OK, wait", recommended: true, picked: true, other: false, note: null },
    { number: 2, label: "Start now anyway", recommended: false, picked: false, other: false, note: null },
  ],
  recommended_label: "Recommended",
  other_placeholder: "Type your answer",
  hint: "",
  in_terminal: "Answer in terminal",
  send: "Send answer",
};

/** The Wishlist epic's sheet as Rust words it (`sheet.view`), its lead asking. */
const WISHLIST: SheetView = {
  ...VIEW,
  key: "SHOP-40",
  title: "Wishlist",
  epic: true,
  answer: ASKS,
  sheet: {
    ...SHEET,
    crumbs: [
      { text: "shop-web", opens: null, key: false },
      { text: "PAY-123", opens: null, key: true },
    ],
    steps: [],
    epic: { progress: "1 of 5 merged", lead: { label: "Lead asks you", tone: "asks_you" } },
    tabs: [
      ...SHEET.tabs.slice(0, 1),
      {
        tab: "tickets",
        label: "Tickets",
        icon: "list",
        number: null,
        keys: null,
        mark: null,
        count: "5",
        contents: ["tickets"],
        lands_on: "tickets",
        parts: [],
      },
    ],
  },
};

const GRID: Grid = {
  heading: "TICKETS · 2",
  corner: "TICKET · NOW",
  columns: ["Ready", "Grill", "Dev"],
  rows: [
    {
      key: "SHOP-40.1",
      short: "40.1",
      title: "Wishlist API",
      line: "Merged · PR #470",
      attention: false,
      cells: [
        { fill: "approved", current: false },
        { fill: "approved", current: false },
        { fill: "approved", current: false },
      ],
    },
    {
      key: "SHOP-40.4",
      short: "40.4",
      title: "Share link",
      line: "Waits for 40.2 · the lead asks you",
      attention: true,
      cells: [
        { fill: "attn", current: true },
        { fill: null, current: false },
        { fill: null, current: false },
      ],
    },
  ],
};

describe("the epic's sheet (SHT-07)", () => {
  it("heads the sheet with its progress and its lead's state, without Move…, and counts its Tickets", () => {
    const out = renderSheet(WISHLIST);
    expect(out).toContain("PAY-123");
    expect(out).toContain("1 of 5 merged");
    expect(out).toContain('data-tone="asks_you"');
    expect(out).toContain("Lead asks you");
    expect(out).not.toContain("Move…");
    expect(out).toContain("Tickets");
    expect(out).toContain('<span class="tab-note">5</span>');
    // Overview's top: what the lead asks, its options as buttons (no radio list).
    expect(out).toContain("Wishlist lead asks");
    expect(out).toContain("OK, wait");
    expect(out).toContain("Start now anyway");
    expect(out).not.toContain('role="radio"');
    // A ticket keeps Move….
    expect(renderSheet(VIEW)).toContain("Move…");
  });

  it("sends the lead the option clicked", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => {
      root.render(createElement(LeadAsks, { view: WISHLIST, answer: ASKS }));
      await Promise.resolve();
    });
    const start = [...host.querySelectorAll("button")].find((b) =>
      b.textContent.includes("Start now anyway"),
    );
    await act(async () => {
      start?.click();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("sheet.answer", {
      key: "SHOP-40",
      question: "q-lead",
      pick: 2,
      other: "",
      clear: false,
    });
    act(() => {
      root.unmount();
    });
  });

  it("draws TICKETS as a stage grid: each child where it is now, what it does", () => {
    const out = renderToStaticMarkup(createElement(EpicGrid, { grid: GRID }));
    for (const text of ["TICKETS · 2", "TICKET · NOW", "Ready", "40.1", "Merged · PR #470", "Share link"]) {
      expect(out).toContain(text);
    }
    expect(out).toContain('data-attention="true"');
    expect(out.match(/data-current="true"/g)).toHaveLength(1);
    expect(out.match(/data-ahead="true"/g)).toHaveLength(2);
  });

  it("lists every child in the Tickets tab, then Add ticket to epic", () => {
    const view: EpicTicketsView = {
      key: "SHOP-40",
      header: { ticket: "TICKET", flow: "WHERE IN THE FLOW", happening: "WHAT’S HAPPENING", who: "WHO" },
      rows: [],
      empty: "No tickets in this epic yet.",
      add: "Add ticket to epic",
      next_key: "· new tickets get the next key, SHOP-40.1",
      note: "When every child is merged…",
    };
    const out = renderToStaticMarkup(createElement(TicketsBody, { view }));
    for (const text of [
      "WHERE IN THE FLOW",
      "No tickets in this epic yet.",
      "Add ticket to epic",
      "SHOP-40.1",
    ]) {
      expect(out).toContain(text);
    }
  });

  it("registers the Tickets tab from its folder", () => {
    expect(registered(registerScreens({ epic }))).toMatchObject({ sheetTabs: ["tickets"] });
  });
});
