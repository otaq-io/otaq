import { createElement, type ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { TicketCardView } from "../generated/TicketCardView";
import { TicketCard } from ".";
import { StatusHoverCard } from "./TicketCard";
import { edgeColor, LOOKS, PALETTES, paletteInk, stateOfLook, toneInk, TONES } from "./look";

const html = (element: ReactElement) => renderToStaticMarkup(element);

// SHOP-22 of *24 · The two views* as `card_page.rs` serves it (BRD-03): one status line, who, the edge, one action.
const SHOP_22: TicketCardView = {
  key: "SHOP-22",
  priority: null,
  title: "Fix flaky login redirect",
  status: { text: "Needs you: allow rm -rf .next?", dot: "attn", tone: "attention", icon: null },
  blocks: null,
  avatar: null,
  who: "Rust developer",
  time: null,
  edge: "attention",
  action: { kind: "open", label: "Answer", icon: null, primary: true },
  origin: null,
  hover: { status: "Needs you · permission", details: ["Tokens: 64k"] },
  faded: null,
  fresh: false,
  open_sheet: "Open panel",
  open_sheet_keys: "↵",
  action_note: null,
  check: null,
  open_terminal: "Open terminal",
};

// SHOP-29: working, no action.
const SHOP_29: TicketCardView = {
  ...SHOP_22,
  key: "SHOP-29",
  title: "Coupon field",
  status: { text: "Working · back from Security", dot: "working", tone: "secondary", icon: null },
  blocks: null,
  time: { text: "8m", stale: false },
  edge: null,
  action: null,
};

describe("every union the card draws has a drawing", () => {
  it("tones, palettes and gallery looks", () => {
    for (const tone of TONES) {
      const { fg, bg } = toneInk(tone);
      expect(fg, tone).toMatch(/^var\(--/);
      expect(bg, tone).toMatch(/^var\(--/);
      expect(edgeColor(tone)).toBe(fg);
    }
    // Each tone has its own ink, except the amber edge of a skip (`paused`), whose words stay calm (FLW-14).
    const own = TONES.filter((t) => t !== "paused");
    expect(new Set(own.map((t) => toneInk(t).fg)).size).toBe(own.length);
    expect(toneInk("paused").fg).toBe(toneInk("warning").fg);
    for (const palette of PALETTES) expect(paletteInk(palette)).toMatch(/^var\(--/);
    expect(new Set(PALETTES.map(paletteInk)).size).toBe(PALETTES.length);
    const states = LOOKS.map((look) => JSON.stringify(stateOfLook(look)));
    expect(new Set(states).size).toBe(LOOKS.length);
  });
});

describe("the one-line card draws its view (BRD-03)", () => {
  it("key, two-line title, one status line with its dot, who, the coloured edge and one action", () => {
    const out = html(createElement(TicketCard, { card: SHOP_22 }));
    expect(out).toContain('<span class="card-key">SHOP-22</span>');
    expect(out).toContain('class="card-title">Fix flaky login redirect</div>');
    expect(out).toContain("Needs you: allow rm -rf .next?");
    expect(out).toContain('class="card-status-dot" style="background:var(--st-attn)"');
    expect(out).toContain('class="card-status-text" style="color:var(--st-attn)"');
    expect(out).toContain('<span class="card-who-name">Rust developer</span>');
    expect(out).toContain("<span>Answer</span>");
    expect(out).toContain('data-edge="attention"');
    expect(out).toContain("--card-outline:var(--st-attn)");
    expect(out).not.toContain("card-quick");
    expect(out).not.toContain("tool-avatar");
    expect(out).not.toContain("status-pill");
  });

  it("a working card: calm words, the time, no edge and no button", () => {
    const out = html(createElement(TicketCard, { card: SHOP_29 }));
    expect(out).toContain('<span class="card-key">SHOP-29</span>');
    expect(out).toContain('style="color:var(--text-secondary)">Working · back from Security</span>');
    expect(out).toContain('<span class="card-time">8m</span>');
    expect(out).not.toContain("data-edge");
    expect(out).not.toContain("card-action");
  });

  it("a calm Backlog card is its key and title only", () => {
    const out = html(
      createElement(TicketCard, {
        card: { ...SHOP_22, status: null, who: null, edge: null, action: null, avatar: null },
      }),
    );
    expect(out).not.toContain("card-status");
    expect(out).not.toContain("card-who");
  });

  it("the hover shows the full status and what the card has no room for, from the key's row", () => {
    const out = html(createElement(StatusHoverCard, { hover: SHOP_22.hover }));
    expect(out).toContain("<strong>Needs you · permission</strong>");
    expect(out).toContain("Tokens: 64k");
  });

  it("an old signal turns amber; a Done card fades", () => {
    const old = html(createElement(TicketCard, { card: { ...SHOP_29, time: { text: "6m", stale: true } } }));
    expect(old).toContain('class="card-time" data-stale="true"');
    const done = html(createElement(TicketCard, { card: { ...SHOP_29, faded: 0.55 } }));
    expect(done).toContain("--card-faded:0.55");
    expect(done).toContain('data-faded="true"');
  });

  it("states: selected, chain-dimmed, hovered with the quick actions", () => {
    const selected = html(
      createElement(TicketCard, { card: SHOP_29, state: { ...stateOfLook("selected") } }),
    );
    expect(selected).toContain('data-selected="true"');
    const dimmed = html(createElement(TicketCard, { card: SHOP_29, state: stateOfLook("dimmed") }));
    expect(dimmed).toContain('data-chain="dimmed"');
    const hovered = html(createElement(TicketCard, { card: SHOP_29, state: stateOfLook("hovered") }));
    expect(hovered).toContain("card-quick");
  });
});
