// @vitest-environment happy-dom
// Pause in the title bar (T-0457; PAU-08, d0112): the slot and ⇧⌘. are registered; the button draws Pause from Rust's
// view, only while agents run; the menu's keys move over the open project and all projects and ↵ picks the highlighted
// one; a pick sends `pause.run` at once (no confirmation) with the open project for its own row.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { createRegistries } from "../../app/registry";
import type { PauseView } from "../../generated/PauseView";
import { pause } from ".";
import { closePauseMenu, menuKey, pauseMenu, runParams, togglePauseMenu } from "./logic";
import { PauseButton, PauseMenuBox } from "./Pause";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

/** `pause.rs`'s words for shop-web with 4 agents working, billing-api with 1. */
const PAUSE: PauseView = {
  shown: true,
  project: "SHOP",
  button: { label: "Pause", icon: "pause", tooltip: "Pause / Resume agents", keys: "⇧⌘." },
  menu: {
    title: "Pause agents",
    rows: [
      {
        scope: "project",
        icon: "pause",
        label: "Pause shop-web",
        detail: "4 agents stop where they are; queued tickets wait.",
        keys: "⇧⌘.",
      },
      {
        scope: "all",
        icon: "layers",
        label: "Pause all projects",
        detail: "5 agents in 2 projects.",
        keys: null,
      },
    ],
    note: "Resume continues each one where it was.",
  },
};

describe("pause", () => {
  it("registers the title bar's slot and ⇧⌘.", () => {
    const app = createRegistries();
    pause(app);
    expect(app.slots.get("header_pause")).toBeDefined();
    expect(app.shortcuts.names()).toEqual(["pause"]);
  });

  it("⇧⌘. and the button toggle the menu", () => {
    closePauseMenu();
    togglePauseMenu();
    expect(pauseMenu.get()).toBe(true);
    togglePauseMenu();
    expect(pauseMenu.get()).toBe(false);
  });

  it("draws Pause with its icon and label", () => {
    const html = renderToStaticMarkup(createElement(PauseButton, { button: PAUSE.button }));
    expect(html).toContain('aria-label="Pause"');
    expect(html).toContain(">Pause<");
  });

  it("draws the menu as Rust words it, the open project highlighted", () => {
    const html = renderToStaticMarkup(createElement(PauseMenuBox, { menu: PAUSE.menu, cursor: 0 }));
    expect(html).toContain("Pause shop-web");
    expect(html).toContain("4 agents stop where they are; queued tickets wait.");
    expect(html).toContain("5 agents in 2 projects.");
    expect(html).toContain("Resume continues each one where it was.");
    expect(html.indexOf("data-highlighted")).toBeLessThan(html.indexOf("Pause all projects"));
  });

  it("↑ ↓ move over the two rows and ↵ picks", () => {
    expect(menuKey("ArrowDown", 0, 2)).toEqual({ kind: "move", index: 1 });
    expect(menuKey("ArrowDown", 1, 2)).toEqual({ kind: "move", index: 0 });
    expect(menuKey("ArrowUp", 0, 2)).toEqual({ kind: "move", index: 1 });
    expect(menuKey("Enter", 0, 2)).toEqual({ kind: "pick", index: 0 });
    expect(menuKey("a", 0, 2)).toEqual({ kind: "pass" });
  });

  it("a pick pauses its scope, the open project's row with the open project", () => {
    const [here, all] = PAUSE.menu.rows;
    if (!here || !all) throw new Error("two rows");
    expect(runParams(here, PAUSE)).toEqual({ scope: "project", project: "SHOP", resume: false });
    expect(runParams(all, { ...PAUSE, project: null })).toEqual({ scope: "all", resume: false });
  });
});
