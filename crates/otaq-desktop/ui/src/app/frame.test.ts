import { createElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import type { LayoutView } from "../generated/LayoutView";
import { Frame, FrameBody } from "./Frame";
import {
  agentPanelWidth,
  INITIAL_LAYOUT,
  layoutView,
  setAgentExpanded,
  setAgentOpen,
  type ShellLayout,
  shellLayout,
  sizeClass,
  toggleExpanded,
} from "./layout";
import { openOverlay, overlays } from "./overlays";
import { createRegistries, type Registries, RegistriesContext } from "./registry";
import { NOTHING_SELECTED, selection } from "./selection";

// `layout.view` as Rust sends it (`layout.rs`).
const VIEW: LayoutView = {
  classes: [
    { class: "w1280", below: 1360 },
    { class: "w1440", below: 1680 },
    { class: "w1920", below: null },
  ],
  rail: 64,
  agent_panel: 420,
  agent_panel_min: 320,
  board_min: 404,
  agent_panel_edge: "Drag to resize. Double-click for the default width.",
};

// The frame renders without a DOM: React's server renderer gives its markup (no new test packages).
function render(app: Registries, bannerFallback?: ReactNode): string {
  return renderToStaticMarkup(
    createElement(RegistriesContext, { value: app }, createElement(Frame, { bannerFallback })),
  );
}

/** The frame's body with `layout`. */
function body(layout: ShellLayout, ticketOpen = false, windowWidth = 0, view = VIEW): string {
  return renderToStaticMarkup(
    createElement(
      RegistriesContext,
      { value: createRegistries() },
      createElement(FrameBody, { view, layout, ticketOpen, windowWidth }),
    ),
  );
}

const slots = (html: string) => [...html.matchAll(/data-slot="([a-z_]+)"/g)].map((m) => m[1]);

afterEach(() => {
  selection.set(NOTHING_SELECTED);
  overlays.store.set([]);
  shellLayout.set(INITIAL_LAYOUT);
  layoutView.set(undefined);
});

describe("the frame", () => {
  it("has rail | Project agent panel | main with the Ask bar, each empty slot showing its name; the ticket sheet only with a ticket open", () => {
    const app = createRegistries();
    // Before `layout.view` answers: no Project agent panel yet.
    expect(slots(render(app))).toEqual([
      "header",
      "rail",
      "banners",
      "board",
      "ask_bar",
      "overlays",
      "toasts",
    ]);

    layoutView.set(VIEW);
    const closed = render(app);
    expect(slots(closed)).toEqual([
      "header",
      "rail",
      "banners",
      "agent_panel",
      "board",
      "ask_bar",
      "overlays",
      "toasts",
    ]);
    for (const name of ["Header", "Rail", "Board"]) expect(closed).toContain(`>${name}</div>`);
    // SHB-01: no Board | Files tab row; the board right at the main area's top.
    expect(closed).not.toContain("frame-tab");
    expect(closed).toContain('<div class="frame-main"><div class="frame-board" data-slot="board">');
    // The header row is the window's drag region, children included (T-0214).
    expect(closed).toContain('<div class="frame-header" data-slot="header" data-tauri-drag-region="deep">');
    expect(closed).not.toContain("Ticket sheet");

    selection.set({ ...NOTHING_SELECTED, ticket: "SHOP-22" });
    const open = render(app);
    expect(slots(open)).toEqual([
      "header",
      "rail",
      "banners",
      "agent_panel",
      "board",
      "sheet",
      "ask_bar",
      "overlays",
      "toasts",
    ]);
    expect(open).toContain(">Ticket sheet</div>");
  });

  it("draws what screens register: slots, and the open overlays that are registered", () => {
    const app = createRegistries();
    app.slots.register("board", { Component: () => createElement("p", null, "the board") });
    app.slots.register("agent_panel", { Component: () => createElement("p", null, "the agent panel") });
    app.slots.register("ask_bar", { Component: () => createElement("p", null, "the bar") });
    app.slots.register("title_bar_end", { Component: () => createElement("p", null, "caption buttons") });
    app.overlays.register("edit_ticket", {
      Component: ({ params }) => createElement("p", null, `editing ${params.ticket}`),
    });
    openOverlay("edit_ticket", { ticket: "SHOP-7" });
    openOverlay("activity");
    layoutView.set(VIEW);

    const html = render(app, createElement("p", { role: "alert" }, "service failed"));
    expect(html).toContain("<p>the board</p>");
    expect(html).toContain("<p>the agent panel</p>");
    expect(html).toContain('<div class="frame-ask" data-slot="ask_bar"><p>the bar</p></div>');
    expect(html).not.toContain(">Board</div>");
    expect(html).toMatch(/<p>caption buttons<\/p><\/div><div class="frame-rail" data-slot="rail"/);
    expect(html).toContain(
      '<div class="frame-layer" id="overlay-host" data-slot="overlays"><p>editing SHOP-7</p></div>',
    );
    expect(html).toContain('<p role="alert">service failed</p>');
  });

  it("gives the banner area to the banners screen once it registers", () => {
    const app = createRegistries();
    app.slots.register("banners", { Component: () => createElement("p", null, "banner") });
    const html = render(app, "service failed");
    expect(html).toContain("<p>banner</p>");
    expect(html).not.toContain("service failed");
  });

  it("the Project agent panel is 420 px while open and stays mounted, hidden, while closed (SHB-01)", () => {
    const closed = body(INITIAL_LAYOUT);
    expect(closed).toContain(
      '<div class="frame-agent" data-slot="agent_panel"><div class="frame-agent-body" style="width:420px" hidden="">',
    );
    const open = body({ ...INITIAL_LAYOUT, agentOpen: true });
    expect(open).toContain(
      '<div class="frame-agent" data-slot="agent_panel" data-open="true"><div class="frame-agent-body" style="width:420px">',
    );
  });

  // T-0502 (dogfood #7).
  it("the panel shows its kept width inside the window's bound, with its edge to drag", () => {
    const open = { ...INITIAL_LAYOUT, agentOpen: true };
    const wide = body(open, false, 1440, { ...VIEW, agent_panel: 640 });
    expect(wide).toContain('<div class="frame-agent-body" style="width:640px">');
    expect(wide).toContain(
      '<div class="frame-agent-edge" aria-hidden="true" title="Drag to resize. Double-click for the default width."></div>',
    );
    // 1280 px leaves the board its 404 px: the panel is at most 812 px.
    expect(body(open, false, 1280, { ...VIEW, agent_panel: 1000 })).toContain('style="width:812px"');
  });

  it("expanded, the panel lies over the board to the window's edge; the column keeps its width, the edge goes", () => {
    const html = body({ ...INITIAL_LAYOUT, agentOpen: true, agentExpanded: true }, false, 1440, {
      ...VIEW,
      agent_panel: 640,
    });
    expect(html).toContain(
      '<div class="frame-agent" data-slot="agent_panel" data-open="true" data-expanded="true" style="width:640px"><div class="frame-agent-body" style="width:1376px">',
    );
    expect(html).not.toContain("frame-agent-edge");
    // Closed, nothing is expanded.
    expect(body({ ...INITIAL_LAYOUT, agentOpen: false, agentExpanded: true })).not.toContain("data-expanded");
  });

  it("the expanded ticket sheet fills the main area while a ticket is open; the agent panel stays", () => {
    const expanded = { ...INITIAL_LAYOUT, panelExpanded: true, agentOpen: true };
    const html = body(expanded, true);
    expect(html).toContain('<div class="frame-main" data-expanded="true">');
    expect(html).toContain('data-slot="agent_panel" data-open="true"');
    // With no ticket open there is nothing to expand.
    expect(body(expanded, false)).not.toContain("data-expanded");
  });
});

describe("the layout's state", () => {
  it("the size class splits at the midpoints, from Rust's table", () => {
    const cases: [number, string][] = [
      [1280, "w1280"],
      [1359, "w1280"],
      [1360, "w1440"],
      [1440, "w1440"],
      [1679, "w1440"],
      [1680, "w1920"],
      [1920, "w1920"],
    ];
    for (const [width, cls] of cases) expect(sizeClass(width, VIEW.classes)).toBe(cls);
    expect(sizeClass(1440, [])).toBeNull();
  });

  it("the panel's width mirrors Rust's bounds (`layout::agent_panel_width`)", () => {
    expect(agentPanelWidth(VIEW, 1440)).toBe(420);
    expect(agentPanelWidth(VIEW, 1440, 200)).toBe(320);
    expect(agentPanelWidth(VIEW, 1440, 1400)).toBe(972);
    expect(agentPanelWidth(VIEW, 700, 500)).toBe(320);
    expect(agentPanelWidth(VIEW, 0, 2000)).toBe(2000);
  });

  it("only an open panel expands, and closing it ends Expand", () => {
    setAgentExpanded(true);
    expect(shellLayout.get().agentExpanded).toBe(false);
    setAgentOpen(true);
    setAgentExpanded(true);
    expect(shellLayout.get()).toMatchObject({ agentOpen: true, agentExpanded: true });
    setAgentOpen(false);
    expect(shellLayout.get()).toMatchObject({ agentOpen: false, agentExpanded: false });
    setAgentOpen(true);
    expect(shellLayout.get().agentExpanded).toBe(false);
  });

  it("the switches flip the page state", () => {
    setAgentOpen(true);
    toggleExpanded();
    expect(shellLayout.get()).toMatchObject({ agentOpen: true, panelExpanded: true });
    setAgentOpen(false);
    expect(shellLayout.get().agentOpen).toBe(false);
  });
});
