import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { DecisionItemView } from "../generated/DecisionItemView";
import type { DecisionsSection } from "../generated/DecisionsSection";
import { DecisionItem, DecisionsList } from ".";

// Hand-written views, as `decision_item.rs` words them (*07 → Row · Decision Item (0084)*).
const LABELS = { built: "Built", other: "Other option", why: "Why", change: "Change…" };

const FULL_NAME: DecisionItemView = {
  id: "SHOP-23#1",
  question: "Split fullName everywhere, or only in the profile form?",
  built: "only in the profile form",
  other: ["everywhere"],
  why: "the description only mentions the profile form.",
  status: null,
  change: true,
  labels: LABELS,
};

const ORDER: DecisionItemView = {
  ...FULL_NAME,
  id: "SHOP-41#1",
  question: "Order of saved items?",
  built: "newest first",
  other: ["in the order they were added"],
  why: "matches the order history page.",
};

const html = (item: DecisionItemView) => renderToStaticMarkup(createElement(DecisionItem, { item }));

describe("the Decision Item draws each variant from its view", () => {
  it("plain, in review: signpost, the question, Built / Other option / Why, and Change…", () => {
    const out = html(FULL_NAME);
    expect(out).toContain("signpost");
    for (const text of [
      FULL_NAME.question,
      "Built",
      "only in the profile form",
      "Other option",
      "everywhere",
      "Why",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).toContain("Change…");
    expect(out).not.toContain("decision-status");
  });

  it("read-only while the ticket works: no Change…", () => {
    expect(html({ ...FULL_NAME, change: false })).not.toContain("Change…");
  });

  it("after Approve and after Change…: the status line with its icon", () => {
    const kept = html({
      ...FULL_NAME,
      change: false,
      status: { text: "Kept by you · Approve, 16:40", icon: "check", kept: true },
    });
    expect(kept).toContain("Kept by you · Approve, 16:40");
    expect(kept).toContain("data-kept");
    const sent = html({
      ...FULL_NAME,
      change: false,
      status: {
        text: "Changed by you → sent back · “Split fullName everywhere”",
        icon: "corner-up-left",
        kept: false,
      },
    });
    expect(sent).toContain("Changed by you → sent back");
    expect(sent).toContain("corner-up-left");
    expect(sent).not.toContain("data-kept");
  });
});

describe("the Decisions list", () => {
  it("a ticket's list: the count, the hint and its items in order", () => {
    const section: DecisionsSection = {
      title: "Decisions",
      count: 2,
      hint: "Approve keeps them all · Change… sends it back",
      groups: [{ items: [FULL_NAME, ORDER] }],
    };
    const out = renderToStaticMarkup(createElement(DecisionsList, { section }));
    expect(out).toContain('class="decisions-count">2<');
    expect(out).toContain("Approve keeps them all · Change… sends it back");
    expect(out.indexOf(FULL_NAME.question)).toBeLessThan(out.indexOf("Order of saved items?"));
  });

  it("no hint after approval", () => {
    const out = renderToStaticMarkup(
      createElement(DecisionsList, {
        section: { title: "Decisions", count: 1, hint: null, groups: [{ items: [FULL_NAME] }] },
      }),
    );
    expect(out).not.toContain("decisions-hint");
  });
});
