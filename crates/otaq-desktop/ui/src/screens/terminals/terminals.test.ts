import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { overlays } from "../../app/overlays";
import { createRegistries, registered, registerScreens, RegistriesContext } from "../../app/registry";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import type { DrawerTab } from "../../generated/DrawerTab";
import type { DrawerView } from "../../generated/DrawerView";
import { DrawerBody } from "./Drawer";
import { openTerminals, terminals } from ".";
import { requestTab, shownTab, showsHint, tabRequests } from "./logic";

afterEach(() => {
  overlays.store.set([]);
  selection.set(NOTHING_SELECTED);
  tabRequests.set(null);
});

const HINT = [
  "Terminal 1 · shop-web · a shell in ~/code/shop-web",
  "Run project commands, or make tickets yourself:",
  '  › otaq create "Wishlist share link"',
  "Want an agent’s help? Ask the Project agent in the Ask bar (⌘L).",
];

const tab = (id: string, label: string, more: Partial<DrawerTab> = {}): DrawerTab => ({
  id,
  label,
  note: null,
  icon: "square-terminal",
  session: `s-${id}`,
  ended: false,
  hint: HINT,
  ended_line: null,
  ...more,
});

const TABS: DrawerTab[] = [
  tab("terminal:t1", "Terminal 1"),
  tab("terminal:t2", "Terminal 2", {
    note: " · exited",
    ended: true,
    hint: [],
    ended_line: "── shell exited 11:20 · scrollback kept · close the tab, or + → New terminal ──",
  }),
];

const VIEW: DrawerView = {
  title: "Terminals",
  tabs: TABS,
  selected: "terminal:t1",
  new_label: "New terminal",
  close_tab: "Close tab",
  hide: "Hide · terminals keep running",
  maximize: "Maximize",
  restore: "Restore",
  empty: ["Nothing open. + → New terminal opens a fresh shell in ~/code/shop-web."],
  height: 380,
};

function render(view: DrawerView, preview = false): string {
  return renderToStaticMarkup(
    createElement(
      RegistriesContext,
      { value: createRegistries() },
      createElement(DrawerBody, {
        view,
        project: "SHOP",
        maximized: false,
        onMaximize: () => undefined,
        onHide: () => undefined,
        preview,
      }),
    ),
  );
}

describe("which tab shows", () => {
  it("the picked one while open, else Rust's choice, else the first", () => {
    expect(shownTab(TABS, "terminal:t2", "terminal:t1")?.label).toBe("Terminal 2");
    expect(shownTab(TABS, "terminal:gone", "terminal:t1")?.label).toBe("Terminal 1");
    expect(shownTab(TABS, null, null)?.label).toBe("Terminal 1");
    expect(shownTab([], null, null)).toBeUndefined();
  });

  it("a shortcut's request counts up", () => {
    requestTab("terminal:t1");
    requestTab("terminal:t1");
    expect(tabRequests.get()).toEqual({ tab: "terminal:t1", seq: 2 });
  });

  it("the hint shows in a live terminal until its first keystroke", () => {
    const t1 = TABS[0] as DrawerTab;
    expect(showsHint(t1, new Set(), true)).toBe(true);
    expect(showsHint(t1, new Set(["terminal:t1"]), true)).toBe(false);
    expect(showsHint(t1, new Set(), false)).toBe(false);
    expect(showsHint(TABS[1] as DrawerTab, new Set(), true)).toBe(false);
  });
});

describe("the drawer", () => {
  it("draws a tab per terminal and + for a new one; no agent, no menu, no Recently ended", () => {
    const html = render(VIEW);
    expect(html.match(/role="tab"/g)).toHaveLength(2);
    expect(html).toContain("Terminal 1");
    expect(html).toContain(" · exited");
    expect(html.match(/aria-label="Close tab"/g)).toHaveLength(1);
    expect(html).toContain('aria-label="New terminal"');
    expect(html).not.toContain("aria-haspopup");
    expect(html).not.toContain("Recently ended");
    expect(html).not.toContain("data-dot");
    expect(html).toContain('aria-label="Terminals"');
    expect(html).toContain("height:380px");
  });

  it("a fresh terminal shows the hint (the gallery draws it as live)", () => {
    const html = render(VIEW, true);
    expect(html).toContain("Run project commands, or make tickets yourself:");
    expect(html).toContain("Ask the Project agent in the Ask bar (⌘L).");
  });

  it("an ended terminal keeps its scrollback with the exit line", () => {
    const html = render({ ...VIEW, selected: "terminal:t2" }, true);
    expect(html).toContain("shell exited 11:20");
    expect(html).not.toContain("Run project commands");
  });

  it("with nothing open it says how to open a terminal", () => {
    const html = render({ ...VIEW, tabs: [], selected: null });
    expect(html).toContain("Nothing open.");
    expect(html).not.toContain('role="tab"');
  });
});

describe("Terminals", () => {
  it("registers the drawer and Terminals from its folder (New Project agent is the agent panel's)", () => {
    const app = registerScreens({ terminals });
    expect(registered(app)).toMatchObject({ overlays: ["terminals"] });
    expect(app.shortcuts.names()).toEqual(["terminals"]);
    // No project shown: the key passes.
    expect(app.shortcuts.run({ action: "terminals" })).toBe(false);
  });

  it("toggles: an open drawer closes", () => {
    const app = registerScreens({ terminals });
    selection.set({ ...NOTHING_SELECTED, project: "SHOP" });
    overlays.open("terminals");
    expect(app.shortcuts.run({ action: "terminals" })).toBe(true);
    expect(overlays.store.get()).toEqual([]);
  });

  it("the Activity popover's Open Terminals only opens: an open drawer stays open", () => {
    registerScreens({ terminals });
    expect(openTerminals()).toBe(false);
    selection.set({ ...NOTHING_SELECTED, project: "SHOP" });
    overlays.open("terminals");
    expect(openTerminals()).toBe(true);
    expect(overlays.store.get().map((o) => o.id)).toEqual(["terminals"]);
  });
});
