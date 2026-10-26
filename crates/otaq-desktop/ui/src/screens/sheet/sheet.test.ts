// @vitest-environment happy-dom
// The ticket sheet (SHT-01, SHT-02; T-0464): drawn to a string, and its keys mounted.
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { shellLayout, INITIAL_LAYOUT } from "../../app/layout";
import { createRegistries, registered, registerScreens, RegistriesContext } from "../../app/registry";
import { NOTHING_SELECTED, openTicket, selection } from "../../app/selection";
import type { SheetView } from "../../generated/SheetView";
import type { SheetTabView } from "../../generated/SheetTabView";
import { sheet } from ".";
import {
  contentOf,
  numberedSheetTab,
  sheetViewLoads,
  sheetWidth,
  shownSheetTab,
  steppedSheetTab,
} from "./logic";
import { useSheetKeys } from "./Sheet";
import { renderSheet as render, SHEET, VIEW } from "./sheet_fixtures";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(),
}));

afterEach(() => {
  selection.set(NOTHING_SELECTED);
  shellLayout.set(INITIAL_LAYOUT);
});

const TABS = SHEET.tabs;

describe("the sheet's tabs (SHT-02)", () => {
  it("opens on Overview; a selected content shows the sheet tab holding it", () => {
    expect(shownSheetTab(TABS, null, "overview")?.tab).toBe("overview");
    expect(shownSheetTab(TABS, "run", "overview")?.tab).toBe("session");
    expect(shownSheetTab(TABS, "files", "overview")?.tab).toBe("files");
    expect(shownSheetTab(TABS, "activity", "overview")?.tab).toBe("activity");
    // Without a Session tab, its agent asked for falls back to the tab it opens on.
    const noSession = TABS.filter((t) => t.tab !== "session");
    expect(shownSheetTab(noSession, "agent", "overview")?.tab).toBe("overview");
    // No check ran: the check asked for falls back too.
    expect(shownSheetTab(TABS, "check", "overview")?.tab).toBe("overview");
  });

  it("Session shows what is picked in it, else the side it lands on (the check while one runs)", () => {
    const session = TABS[1];
    if (!session) throw new Error("no Session");
    expect(contentOf(session, null)).toBe("agent");
    expect(contentOf(session, "shell")).toBe("shell");
    expect(contentOf(session, "files")).toBe("agent");
    const checking: SheetTabView = {
      ...session,
      contents: ["agent", "check", "run", "shell"],
      lands_on: "check",
    };
    expect(contentOf(checking, null)).toBe("check");
    expect(contentOf(checking, "agent")).toBe("agent");
  });

  it("1–4 (g o / s / f / a) pick by number; ⌃Tab steps round the ends", () => {
    expect(numberedSheetTab(TABS, 4)?.tab).toBe("activity");
    expect(numberedSheetTab(TABS, 5)).toBeUndefined();
    expect(steppedSheetTab(TABS, TABS[3], 1)?.tab).toBe("overview");
    expect(steppedSheetTab(TABS, TABS[0], -1)?.tab).toBe("activity");
    expect(steppedSheetTab([], undefined, 1)).toBeUndefined();
  });

  it("is 1040 px, 920 px beside Noyan, and never wider than the main area less its margins", () => {
    expect(sheetWidth(SHEET, false, 1376)).toBe(1040);
    expect(sheetWidth(SHEET, true, 976)).toBe(920);
    expect(sheetWidth(SHEET, true, 896)).toBe(860);
  });
});

describe("the sheet", () => {
  it("draws the breadcrumb, title, status, Move…, the details line, the path and four tabs", () => {
    const html = render(VIEW);
    for (const text of [
      "shop-web",
      "SHOP-19",
      "Price table",
      "Working",
      "Move…",
      "Assignee",
      "Rust developer",
      "Claude Code · sonnet",
      "3 min · 182k tokens",
      "otaq/SHOP-19",
      "from main",
      "Product spec",
    ]) {
      expect(html).toContain(text);
    }
    expect(html.match(/role="tab"/g)).toHaveLength(4);
    expect(html).toContain('data-tab="overview"');
    expect(html).toContain('aria-current="step"');
    expect(html).toContain('aria-label="Priority: P3"');
    expect(html).toContain('aria-label="Full page"');
    expect(html).toContain("width:1040px");
  });

  it("shows the selected tab's body: Session's switch over Run, Activity; Files empty until its screen", () => {
    const session = render(VIEW, { tab: "run" });
    expect(session).toContain('data-tab="session"');
    expect(session.match(/role="radio"/g)).toHaveLength(3);
    expect(session).toContain('aria-checked="true">Run');
    expect(session).toContain('Agent<span class="session-dot" data-dot="running">');
    expect(render(VIEW, { tab: "activity" })).toContain("activity of SHOP-19");
    expect(render(VIEW, { tab: "files" })).toContain('<div class="sheet-body" data-tab="files"></div>');
  });

  it("at full page: ← Board in place of the project, no fixed width, and the button goes back", () => {
    const html = render(VIEW, { fullPage: true });
    expect(html).toContain("data-full-page");
    expect(html).toContain("Board");
    expect(html).not.toContain("shop-web");
    expect(html).toContain('aria-label="Back to the sheet"');
    expect(html).not.toContain("width:1040px");
  });

  it("the External ID copies on a click; the Files tab shows its count (Q416, Q421)", () => {
    const html = render({
      ...VIEW,
      sheet: {
        ...SHEET,
        details: [
          ...SHEET.details,
          {
            label: "External ID",
            value: "PAY-123",
            avatar: null,
            working: false,
            mono: true,
            note: null,
            change: { kind: "copy", text: "PAY-123", hint: "Copy External ID", copied: "Copied" },
          },
        ],
        tabs: SHEET.tabs.map((t) => (t.tab === "files" ? { ...t, count: "3" } : t)),
      },
    });
    expect(html).toContain('aria-label="External ID: PAY-123. Copy External ID"');
    expect(html).toContain('<span class="sheet-detail-value" data-mono="true">PAY-123</span>');
    expect(html).toMatch(/Files<\/span><span class="tab-note">3<\/span>/);
  });

  it("only the first load of an opening takes the Files count again (Q421)", () => {
    const loads = sheetViewLoads("SHOP-19", true);
    expect(loads()).toEqual({ key: "SHOP-19", opened: true });
    expect(loads()).toEqual({ key: "SHOP-19", opened: false });
    expect(loads()).toEqual({ key: "SHOP-19", opened: false });
    // Sliding out: nothing is taken.
    expect(sheetViewLoads("SHOP-19", false)()).toEqual({ key: "SHOP-19", opened: false });
  });

  it("fills the sheet slot and registers the ticket's dialogs; each tab's body is its own screen's", () => {
    const app = registerScreens({ sheet });
    expect(registered(app)).toMatchObject({
      slots: ["sheet"],
      overlays: ["close_ticket", "submit_for_review"],
      sheetTabs: [],
    });
  });
});

describe("the sheet's keys (0097, SHT-02)", () => {
  function mount(view: SheetView) {
    const app = createRegistries();
    const host = document.body.appendChild(document.createElement("div"));
    const root = createRoot(host);
    function Probe() {
      useSheetKeys(view);
      return null;
    }
    act(() => {
      root.render(createElement(RegistriesContext, { value: app }, createElement(Probe)));
    });
    return {
      app,
      done: () => {
        act(() => {
          root.unmount();
        });
        host.remove();
      },
    };
  }

  it("tab keys, ⌃Tab, j / k keeping the tab, e and Esc", () => {
    openTicket("SHOP-19");
    const { app, done } = mount(VIEW);
    expect(app.shortcuts.run({ action: "switch_tab", number: 2 })).toBe(true);
    expect(selection.get().tab).toBe("agent");
    expect(app.shortcuts.run({ action: "switch_tab", number: 4 })).toBe(true);
    expect(selection.get().tab).toBe("activity");
    expect(app.shortcuts.run({ action: "switch_tab", number: 5 })).toBe(false);
    app.shortcuts.run({ action: "next_tab" });
    expect(selection.get().tab).toBe("overview");
    app.shortcuts.run({ action: "next_ticket" });
    expect(selection.get()).toMatchObject({ ticket: "SHOP-20", tab: "overview" });
    expect(app.shortcuts.run({ action: "open_diff" })).toBe(true);
    expect(selection.get().tab).toBe("files");
    app.shortcuts.run({ action: "expand_sheet" });
    expect(shellLayout.get().sheetExpanded).toBe(true);
    app.shortcuts.run({ action: "close_sheet" });
    expect(selection.get().ticket).toBeNull();
    done();
  });

  it("Esc in Noyan's panel is left to the agent panel; the sheet stays open", () => {
    openTicket("SHOP-19");
    const { app, done } = mount(VIEW);
    const panel = document.body.appendChild(document.createElement("div"));
    panel.dataset.slot = "agent_panel";
    const input = panel.appendChild(document.createElement("textarea"));
    input.focus();
    expect(app.shortcuts.run({ action: "close_sheet" })).toBe(false);
    expect(selection.get().ticket).toBe("SHOP-19");
    input.blur();
    panel.remove();
    expect(app.shortcuts.run({ action: "close_sheet" })).toBe(true);
    expect(selection.get().ticket).toBeNull();
    done();
  });

  it("g a is Activity's popover while no sheet takes it", () => {
    const { app, done } = mount({ ...VIEW, sheet: { ...SHEET, tabs: [] } });
    expect(app.shortcuts.run({ action: "switch_tab", number: 4 })).toBe(false);
    done();
  });
});
