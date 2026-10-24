// SHOP-19 "Price table" working in Dev, as Rust words its sheet (`panel.view`), for the sheet's and its actions' tests,
// and the sheet drawn to a string with Overview, Session and an Activity and a Stack stand-in registered.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { createRegistries, RegistriesContext, type Registries } from "../../app/registry";
import type { PanelView } from "../../generated/PanelView";
import type { SheetContent } from "../../generated/SheetContent";
import type { SheetTabView } from "../../generated/SheetTabView";
import type { SheetView } from "../../generated/SheetView";
import { overview as overviewScreen } from "../overview";
import { session as sessionScreen } from "../session";
import { sheet as sheetScreen } from ".";
import { SheetBody } from "./Sheet";

const hint = (text: string, keys: string | null = null) => ({ text, keys });

const tab = (
  t: SheetTabView["tab"],
  label: string,
  number: number,
  contents: SheetContent[],
): SheetTabView => ({
  tab: t,
  label,
  icon: "file-text",
  number,
  keys: String(number),
  mark: null,
  count: null,
  contents,
  lands_on: contents[0] ?? "overview",
  parts: [],
});

export const SHEET: SheetView = {
  crumbs: [
    { text: "shop-web", opens: null, key: false },
    { text: "SHOP-19", opens: null, key: true },
  ],
  details: [
    {
      label: "Assignee",
      value: "Rust developer",
      avatar: null,
      working: false,
      mono: false,
      note: null,
      change: null,
    },
    {
      label: "Model",
      value: "Claude Code · sonnet",
      avatar: null,
      working: false,
      mono: false,
      note: null,
      change: null,
    },
    {
      label: "Working",
      value: "3 min · 182k tokens",
      avatar: null,
      working: true,
      mono: false,
      note: null,
      change: null,
    },
    {
      label: "Priority",
      value: "P3",
      avatar: null,
      working: false,
      mono: false,
      note: null,
      change: { kind: "priority", items: [] },
    },
    {
      label: "Branch",
      value: "otaq/SHOP-19",
      avatar: null,
      working: false,
      mono: true,
      note: "from main",
      change: null,
    },
  ],
  epic: null,
  steps: [
    { stage: "Grill", state: "done", attention: false },
    { stage: "Dev", state: "current", attention: false },
    { stage: "Review", state: "to_come", attention: false },
  ],
  tabs: [
    tab("overview", "Overview", 1, ["overview"]),
    {
      ...tab("session", "Session", 2, ["agent", "run", "shell"]),
      mark: { kind: "live" },
      parts: [
        { content: "agent", label: "Agent", dot: "running" },
        { content: "run", label: "Run", dot: null },
        { content: "shell", label: "Shell", dot: null },
      ],
    },
    tab("files", "Files", 3, ["files"]),
    tab("activity", "Activity", 4, ["activity"]),
  ],
  opens_on: "overview",
  move_label: "Move…",
  full_page: hint("Full page", "e"),
  back: hint("Back to the sheet", "e"),
  board: "Board",
  width: 1040,
  width_beside_agent: 920,
  margin: 18,
};

export const VIEW: PanelView = {
  key: "SHOP-19",
  title: "Price table",
  epic: false,
  priority: {
    number: 3,
    short: "P3",
    word: "Normal",
    label: "P3 · Normal",
    icon: "chevron-up",
    ink: "muted",
  },
  priority_hint: "Priority",
  priority_menu: [],
  pill: { palette: "working", icon: "loader", label: "Working" },
  branch: { shown: "otaq/SHOP-19", full: "otaq/SHOP-19" },
  copy_branch: "Copy branch name",
  base: "main",
  signal: { text: "40s ago", stale: false },
  tokens: { text: "182k", tooltip: null },
  token_split: null,
  previous: null,
  next: "SHOP-20",
  close_hint: hint("Close", "Esc"),
  more_hint: "More",
  buttons: [
    {
      action: "open_terminal",
      label: "Open terminal",
      tooltip: null,
      icon: "square-terminal",
      look: "secondary",
    },
  ],
  approve_more: null,
  approve_opens_menu: false,
  stop_message: null,
  answer_agent: null,
  parent: null,
  menu: [],
  attach_command: "otaq attach SHOP-19",
  answer: null,
  notice: null,
  conflict: null,
  start_problem: null,
  mark_closed: null,
  run: null,
  sheet: SHEET,
  live_agent: true,
  choices: null,
  path: null,
  open_agent: null,
  build_aside: null,
  refusal: null,
  holds: [],
};

/** Registries with the sheet's, Overview's and Session's screens, and Activity's and the Dependencies section's
 * stand-ins. */
export function sheetRegistries(): Registries {
  const app = createRegistries();
  sheetScreen(app);
  overviewScreen(app);
  sessionScreen(app);
  app.sheetTabs.register("activity", {
    Component: ({ ticket }) => createElement("p", null, `activity of ${ticket}`),
  });
  app.panelSections.register("dependencies", {
    Component: () => createElement("p", null, "the dependencies"),
  });
  return app;
}

/** The sheet for `view` drawn to a string, on `tab` (`null`: the tab it opens on). */
export function renderSheet(
  view: PanelView,
  opts: { tab?: SheetContent | null; fullPage?: boolean; width?: number } = {},
): string {
  return renderToStaticMarkup(
    createElement(
      RegistriesContext,
      { value: sheetRegistries() },
      createElement(SheetBody, {
        view,
        tab: opts.tab ?? null,
        fullPage: opts.fullPage ?? false,
        width: opts.width ?? 1040,
      }),
    ),
  );
}
