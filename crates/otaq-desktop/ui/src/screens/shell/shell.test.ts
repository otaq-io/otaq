import { createElement, isValidElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { overlays } from "../../app/overlays";
import { createRegistries, registered, registerScreens } from "../../app/registry";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import type { AskAgentButton } from "../../generated/AskAgentButton";
import type { ActivityPopover } from "../../generated/ActivityPopover";
import type { HeaderView } from "../../generated/HeaderView";
import type { QuickSwitcherView } from "../../generated/QuickSwitcherView";
import type { RailItem } from "../../generated/RailItem";
import type { RailView } from "../../generated/RailView";
import { ActivityBody, LimitsBody } from "./ActivityPopover";
import { ActivityChip, HeaderBar } from "./Header";
import { shell, switchToDigit } from ".";
import { clampCursor, switcherKey } from "./logic";
import { QuickList } from "./QuickSwitcher";
import { railActions, RailTiles } from "./Rail";

afterEach(() => {
  overlays.store.set([]);
  selection.set(NOTHING_SELECTED);
});

const hint = (text: string, keys: string | null) => ({ text, keys });

const HEADER: HeaderView = {
  project: {
    key: "SHOP",
    name: "shop-web",
    base_branch: "main",
    path: "~/code/shop-web",
    repo_path: "/code/shop-web",
    menu: {
      title: "shop-web",
      path: "~/code/shop-web",
      items: [],
      export_title: "Export shop-web’s flow",
      export_file: "shop-web-flow.yaml",
    },
    flow_setup: null,
  },
  search: hint("Search  ·  is:needs-you", "/"),
  activity: {
    agents: { count: "3", label: "running", control: false, active: true, paused: false },
    processes: "3",
    warning: { text: "5h 18% left", token: "st-paused", limit: false },
    hover: {
      title: "3 agents running across all projects",
      lines: ["shop-web 2 · billing-api 1 · 3 processes"],
    },
    label: "Activity",
    keys: "⇧⌘U",
  },
};

/** Every element with a click handler in a tree of elements, walked without a DOM, by its label or class. */
function clicksOf(root: ReactNode): Map<string, () => void> {
  const clicks = new Map<string, () => void>();
  const walk = (node: ReactNode, name: string) => {
    if (Array.isArray(node)) {
      for (const n of node as ReactNode[]) walk(n, name);
      return;
    }
    if (!isValidElement(node)) return;
    const props = node.props as {
      className?: string;
      label?: string;
      "aria-label"?: string;
      onClick?: () => void;
      children?: ReactNode;
    };
    const here = props.label ?? props["aria-label"] ?? props.className ?? name;
    if (props.onClick) clicks.set(here, props.onClick);
    walk(props.children, here);
  };
  walk(root, "");
  return clicks;
}

const item = (key: string, name: string, extra: Partial<RailItem> = {}): RailItem => ({
  key,
  name,
  repo_path: `/code/${name}`,
  title: `${name} · ${key}`,
  path: `~/code/${name}`,
  agents_working: 0,
  needs_you: 0,
  selected: false,
  idle: true,
  ringing: false,
  shortcut: null,
  detail: ["idle"],
  note: null,
  flow_setup: null,
  paused: null,
  ...extra,
});

const RAIL: RailView = {
  selected: "SHOP",
  items: [
    item("SHOP", "shop-web", {
      selected: true,
      idle: false,
      needs_you: 6,
      agents_working: 2,
      shortcut: "⌘1",
    }),
    item("BILL", "billing-api", { idle: false, needs_you: 1, ringing: true }),
    item("CREW", "otaq"),
  ],
  width: 64,
  ring_ends_in_ms: 4000,
  switch_keys: "⌘P",
  bottom: { terminals: { label: "Terminals", keys: "⌃`" }, settings: { label: "Settings", keys: "⌘," } },
  project_menu: [
    ["project_settings", "Project settings…"],
    ["rename_project", "Rename…"],
    ["remove_project", "Remove project…"],
  ].map(([id, label]) => ({
    id: id ?? "",
    label: label ?? "",
    icon: null,
    shortcut: null,
    danger: id === "remove_project",
    disabled: false,
    hint: null,
    separated: false,
    checked: false,
    header: false,
  })),
};

describe("the quick switcher's keys", () => {
  it("move with wrapping, switch on ↵, close on Esc, and leave typing to the field", () => {
    expect(switcherKey("ArrowDown", 2, 3)).toEqual({ kind: "move", index: 0 });
    expect(switcherKey("ArrowUp", 0, 3)).toEqual({ kind: "move", index: 2 });
    expect(switcherKey("ArrowDown", 0, 0)).toEqual({ kind: "move", index: 0 });
    expect(switcherKey("Enter", 1, 3)).toEqual({ kind: "choose", index: 1 });
    expect(switcherKey("Enter", 5, 2)).toEqual({ kind: "choose", index: 1 });
    expect(switcherKey("Enter", 0, 0)).toEqual({ kind: "pass" });
    expect(switcherKey("Escape", 0, 3)).toEqual({ kind: "close" });
    expect(switcherKey("b", 0, 3)).toEqual({ kind: "pass" });
  });

  it("keep the cursor on a row when the list shrinks", () => {
    expect(clampCursor(4, 2)).toBe(1);
    expect(clampCursor(3, 0)).toBe(0);
    expect(clampCursor(1, 5)).toBe(1);
  });
});

describe("the rail", () => {
  it("draws the badges, the selection, a ringing tile and quiet idle projects", () => {
    const out = renderToStaticMarkup(createElement(RailTiles, { view: RAIL }));
    expect(out).toContain('data-selected="true"');
    expect(out).toContain('<span class="rail-needs">6</span>');
    expect(out).toContain('<span class="rail-tile" data-ringing="true">BILL');
    expect(out).toMatch(/data-idle="true"[^>]*aria-label="otaq · CREW"/);
    expect(out).toContain('aria-label="Add project"');
  });

  it("ends with the Project agent, Terminals, Cleanup and Settings, which open their screens as Terminals and Settings do", () => {
    const agent: AskAgentButton = {
      icon: "bot",
      label: "Project agent",
      hint: "Open or close · ⌘L",
      tinted: false,
      dot: true,
    };
    const out = renderToStaticMarkup(createElement(RailTiles, { view: RAIL, agent }));
    const order = [
      'aria-label="Project agent"',
      'class="rail-rule"',
      'aria-label="Terminals"',
      'aria-label="Cleanup"',
      'aria-label="Settings"',
    ].map((t) => out.indexOf(t));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect(order).toEqual(order.toSorted((a, b) => a - b));
    // SHB-02: a dot while a reply waits; tinted only while the panel is open.
    expect(out).toContain('<span class="rail-agent-dot"></span>');
    expect(out).not.toContain("data-tinted");
    const open = renderToStaticMarkup(
      createElement(RailTiles, { view: RAIL, agent: { ...agent, tinted: true, dot: false } }),
    );
    expect(open).toContain('aria-pressed="true" data-tinted="true"');
    expect(open).not.toContain("rail-agent-dot");
    // Before the Ask bar loaded: no button yet.
    expect(renderToStaticMarkup(createElement(RailTiles, { view: RAIL }))).not.toContain("rail-agent");

    const app = createRegistries();
    const ran: string[] = [];
    app.shortcuts.on("terminals", () => {
      ran.push("terminals");
    });
    app.shortcuts.on("settings", () => {
      ran.push("settings");
    });
    const tiles = RailTiles({ view: RAIL, actions: railActions(app) });
    // The bottom buttons sit inside their tooltips: walk to them by their labels.
    const clicks = clicksOf(tiles);
    for (const name of ["Terminals", "Settings"]) {
      const click = clicks.get(name);
      expect(click, name).toBeDefined();
      click?.();
    }
    expect(ran).toEqual(["terminals", "settings"]);
  });
});

describe("the rail's project menu (#11)", () => {
  it("opens at the pointer with the project's name and folder, then Project settings, Rename and Remove", () => {
    const project = RAIL.items[0];
    if (!project) throw new Error("a project");
    const out = renderToStaticMarkup(
      createElement(RailTiles, {
        view: RAIL,
        menu: { project, anchor: { left: 30, top: 80, width: 0, height: 0 } },
      }),
    );
    for (const text of ["shop-web", "Project settings…", "Rename…", "Remove project…"])
      expect(out).toContain(text);
    expect(renderToStaticMarkup(createElement(RailTiles, { view: RAIL }))).not.toContain("Rename…");
  });
});

describe("a project in Flow setup (T-0436, `s14-18`)", () => {
  it("the rail marks it Flow setup under its name", () => {
    const view: RailView = {
      ...RAIL,
      items: [
        item("SHOP", "shop-web"),
        item("BILL", "billing-api", { flow_setup: "Flow setup", detail: ["Flow setup"] }),
      ],
    };
    const out = renderToStaticMarkup(createElement(RailTiles, { view }));
    expect(out.match(/data-flow-setup="true"/g)).toHaveLength(1);
    expect(out).toMatch(/data-flow-setup="true"[^>]*aria-label="billing-api · BILL"/);
    expect(out).toContain(
      '<span class="rail-name">billing-api</span><span class="rail-flow-setup">Flow setup</span>',
    );
  });

  const inFlowSetup = (label: string | null): HeaderView => ({
    ...HEADER,
    project: {
      ...(HEADER.project as NonNullable<HeaderView["project"]>),
      key: "BILL",
      name: "billing-api",
      flow_setup: {
        label: "Flow setup",
        status: label === null ? null : { palette: "working", icon: "loader-circle", label },
      },
    },
  });

  it("the title bar reads billing-api · Flow setup · 2 agents at work, with no branch and no search", () => {
    const out = renderToStaticMarkup(
      createElement(HeaderBar, { view: inFlowSetup("2 agents at work"), onSearch: () => undefined }),
    );
    const order = ["billing-api", "·", "Flow setup", "2 agents at work"].map((text) => out.indexOf(text));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect(order).toEqual(order.toSorted((a, b) => a - b));
    expect(out).not.toContain("git-branch");
    expect(out).not.toContain("header-search");
    expect(out).toContain('aria-label="Activity"');
  });

  it("nothing asked yet: Flow setup alone", () => {
    const out = renderToStaticMarkup(
      createElement(HeaderBar, { view: inFlowSetup(null), onSearch: () => undefined }),
    );
    expect(out).toContain("Flow setup");
    expect(out).not.toContain("at work");
  });
});

describe("the header", () => {
  it("draws the three items, left to right, and nothing else", () => {
    const out = renderToStaticMarkup(createElement(HeaderBar, { view: HEADER, onSearch: () => undefined }));
    const order = ["shop-web", "main", "Search  ·  is:needs-you", 'aria-label="Activity"'].map((text) =>
      out.indexOf(text),
    );
    expect(order.every((at) => at >= 0)).toBe(true);
    expect(order).toEqual(order.toSorted((a, b) => a - b));
    // The path is the name's hover, not a label; New ticket, Terminals and Settings are on the rail or ⌘N.
    expect(out).not.toContain("~/code/shop-web");
    // Needs you is the board's strip (d0114): no Inbox button.
    for (const gone of ["New ticket", "Terminals", "Settings", "tokens", "Inbox", "header-inbox"])
      expect(out).not.toContain(gone);
  });

  it("Activity reads its counts and draws the warning only when the view has one, in the view's amber", () => {
    const out = renderToStaticMarkup(createElement(HeaderBar, { view: HEADER, onSearch: () => undefined }));
    const order = ["running", "▷ 3", "5h 18% left"].map((text) => out.indexOf(text));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect(order).toEqual(order.toSorted((a, b) => a - b));
    expect(out).toContain('class="header-warning" style="color:var(--st-paused)"');
    expect(out).not.toContain("st-attn");
    expect(out).not.toContain("data-limit");

    const atLimit = renderToStaticMarkup(
      createElement(HeaderBar, {
        view: {
          ...HEADER,
          activity: { ...HEADER.activity, warning: { text: "5h limit", token: "st-paused", limit: true } },
        },
        onSearch: () => undefined,
      }),
    );
    expect(atLimit).toContain('data-limit="true"');
    expect(atLimit).toContain("5h limit");

    const calm = renderToStaticMarkup(
      createElement(HeaderBar, {
        view: { ...HEADER, activity: { ...HEADER.activity, warning: null } },
        onSearch: () => undefined,
      }),
    );
    expect(calm).not.toContain("header-warning");
    expect(calm).not.toContain("18% left");
  });

  it("search and Activity open what they open", () => {
    let searched = 0;
    const bar = clicksOf(HeaderBar({ view: HEADER, onSearch: () => (searched += 1) }));
    bar.get("header-search")?.();
    expect(searched).toBe(1);
    clicksOf(ActivityChip({ activity: HEADER.activity })).get("Activity")?.();
    expect(overlays.store.get().map((o) => o.id)).toEqual(["activity"]);
  });

  it("hides what has no data, and reads slots with limits on", () => {
    const quiet: HeaderView = {
      ...HEADER,
      project: null,
      activity: {
        ...HEADER.activity,
        agents: { count: "0/5", label: "agents", control: true, active: false, paused: false },
        processes: null,
        warning: null,
      },
    };
    const out = renderToStaticMarkup(createElement(HeaderBar, { view: quiet, onSearch: () => undefined }));
    expect(out).not.toContain("header-search");
    expect(out).not.toContain("▷");
    expect(out).not.toContain("header-warning");
    expect(out).toContain("0/5");
  });
});

const LIMITS = {
  title: "Agent limits",
  max_agents: { label: "Max agents running at once", value: 1, can_lower: false, can_raise: true },
  back_pressure: {
    label: "Pause new starts when Needs you has",
    value: 3,
    can_lower: true,
    can_raise: true,
  },
  note: "Tickets in Ready for Dev wait for a free slot.",
};

const POPOVER: ActivityPopover = {
  limits: null,
  agents: {
    title: "AGENTS RUNNING",
    count: "3",
    rows: [],
    empty: null,
  },
  usage: {
    title: "USAGE · WHAT'S LEFT",
    today: "Today 1.21M tokens",
    tools: [
      {
        name: "Claude Code",
        avatar: null,
        tokens: "1.21M today",
        windows: [
          { label: "5h", left: "18% left", reset: "resets 3:45 pm", fill: 18, amber: true },
          { label: "week", left: "60% left", reset: "resets Mon", fill: 60, amber: false },
        ],
        notes: [],
      },
    ],
  },
  processes: {
    title: "PROCESSES",
    count: "1",
    list: {
      title: "Run processes",
      stop_all: "Stop all",
      stop: "Stop",
      empty: "Nothing running",
      rows: [
        {
          project_name: "pocket-notes",
          key: "NOTE-5",
          title: "Sync",
          port: ":4020",
          uptime: null,
          url: "http://localhost:4020",
          open: "Open :4020",
        },
      ],
    },
  },
  open_terminals: { label: "Open Terminals", keys: "⌃`" },
};

describe("the Activity popover", () => {
  const draw = (view: ActivityPopover) =>
    renderToStaticMarkup(
      createElement(ActivityBody, {
        view,
        onStep: () => undefined,
        onOpenTicket: () => undefined,
        onOpenTerminals: () => undefined,
      }),
    );

  it("lists agents, what's left per window with its bar, the processes and Open Terminals, in order", () => {
    const out = draw(POPOVER);
    const order = [
      "AGENTS RUNNING",
      "USAGE · WHAT&#x27;S LEFT",
      "Claude Code",
      "18% left",
      "resets 3:45 pm",
      "60% left",
      "PROCESSES",
      "NOTE-5",
      "Stop all",
      "Open Terminals",
      "⌃`",
    ].map((text) => out.indexOf(text));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect(order.slice(0, 6)).toEqual(order.slice(0, 6).toSorted((a, b) => a - b));
    expect(out).toContain('style="width:18%"');
    expect(out.match(/class="activity-window" data-amber="true"/g)).toHaveLength(1);
    // PROCESSES heads the reused rows in place of their own title.
    expect(out).not.toContain("Run processes");
    expect(out).not.toContain("$");
  });

  it("puts the stepper first with limits on, and says so when nothing runs", () => {
    const out = draw({
      ...POPOVER,
      limits: LIMITS,
      agents: {
        ...POPOVER.agents,
        count: "0",
        empty: "Nothing running. Tickets in Ready for Dev start when a slot allows.",
      },
      processes: null,
    });
    expect(out.indexOf("Agent limits")).toBeLessThan(out.indexOf("AGENTS RUNNING"));
    expect(out).toContain("Nothing running. Tickets in Ready for Dev start");
    expect(out).not.toContain("PROCESSES");
  });

  it("the stepper is disabled at its bound", () => {
    const out = renderToStaticMarkup(createElement(LimitsBody, { view: LIMITS, onStep: () => undefined }));
    expect(out).toContain('aria-label="Fewer" disabled=""');
    expect(out.match(/disabled=""/g)).toHaveLength(1);
  });
});

describe("the quick switcher's list", () => {
  it("highlights the cursor's row and shows the footer hints", () => {
    const view: QuickSwitcherView = {
      placeholder: "Switch project…",
      rows: [
        { project: RAIL.items[0] as RailItem, label: "shop-web · SHOP · current", running: "2 running" },
        { project: RAIL.items[2] as RailItem, label: "otaq · CREW", running: null },
      ],
      hints: [
        { keys: "↑↓", label: "move" },
        { keys: "↵", label: "switch" },
        { keys: "⌘N", label: "add project" },
      ],
    };
    const out = renderToStaticMarkup(
      createElement(QuickList, { view, cursor: 1, onChoose: () => undefined }),
    );
    expect(out).toMatch(/aria-selected="false"[^>]*>.*shop-web · SHOP · current/);
    expect(out).toMatch(/aria-selected="true" data-idle="true"/);
    expect(out).toContain("2 running");
    expect(out).toContain("add project");
  });
});

describe("the shell screen", () => {
  it("fills the header and the rail and owns ⌘P, ⌘1–9 and the Activity popover", () => {
    const app = registerScreens({ shell });
    expect(registered(app)).toMatchObject({
      slots: ["header", "rail"],
      overlays: ["quick_switcher", "activity"],
      shortcuts: ["open_activity", "project", "switch_project"],
    });
    expect(app.shortcuts.run({ action: "switch_project" })).toBe(true);
    expect(overlays.store.get()).toEqual([{ id: "quick_switcher", params: undefined }]);
    app.shortcuts.run({ action: "switch_project" });
    expect(overlays.store.get()).toEqual([]);
  });

  it("⇧⌘U toggles the Activity popover", () => {
    const app = registerScreens({ shell });
    expect(app.shortcuts.run({ action: "open_activity" })).toBe(true);
    expect(overlays.store.get()).toEqual([{ id: "activity", params: undefined }]);
    app.shortcuts.run({ action: "open_activity" });
    expect(overlays.store.get()).toEqual([]);
  });

  it("⌘1–9 switch to the rail's project at that place, when there is one", async () => {
    const asked: unknown[] = [];
    const ask = (method: string, params: unknown) => {
      asked.push([method, params]);
      return Promise.resolve((params as { digit: number }).digit === 2 ? "BILL" : null);
    };
    expect(await switchToDigit(2, ask)).toBe("BILL");
    expect(selection.get().project).toBe("BILL");
    expect(await switchToDigit(7, ask)).toBe(null);
    expect(selection.get().project).toBe("BILL");
    expect(asked).toEqual([
      ["shell.project_for_digit", { digit: 2 }],
      ["shell.project_for_digit", { digit: 7 }],
    ]);
  });
});
