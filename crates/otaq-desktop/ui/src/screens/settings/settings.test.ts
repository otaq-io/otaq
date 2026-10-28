// @vitest-environment happy-dom
import { createElement, type ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { overlays } from "../../app/overlays";
import { registerScreens } from "../../app/registry";
import type { GlobalSectionId } from "../../generated/GlobalSectionId";
import type { SettingsNavRow } from "../../generated/SettingsNavRow";
import type { SettingsPage } from "../../generated/SettingsPage";
import type { SettingsSection } from "../../generated/SettingsSection";
import { settings } from ".";
import { SettingsContext, sectionOf, topicRow } from "./context";
import { Section, sectionComponent } from "./Rows";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(() => Promise.resolve(null)),
  pickFolder: vi.fn(),
}));
import type { AgentToolRow } from "../../generated/AgentToolRow";
import type { AgentToolsPage } from "../../generated/AgentToolsPage";
import { agentToolMenu, AgentToolsPageBody, DisconnectDialogView, markOf, stepOf } from "./AgentTools";

const html = (element: ReactElement) => renderToStaticMarkup(element);

afterEach(() => {
  overlays.store.set([]);
});

// The shape `settings_page.rs` sends for *Agent limits* while they are off.
const LIMITS: SettingsSection<"agent_limits"> = {
  id: "agent_limits",
  title: "Agent limits",
  intro: "Optional caps on how many agents run at once.",
  rows: [
    {
      field: "agent_limits.enabled",
      label: "Limit agents",
      description: "Off by default. When on, the header shows “3/5 agents”.",
      control: { kind: "toggle", on: false },
      dimmed: false,
      reset: null,
      button: null,
    },
    {
      field: "agent_limits.max_concurrency",
      label: "Max agents running at once",
      description: "Suggested 5. Tickets in Ready for Dev wait for a free slot.",
      control: { kind: "stepper", value: 5, min: 1, max: 50, suffix: "", disabled: true, empty: null },
      dimmed: true,
      reset: null,
      button: null,
    },
  ],
  note: null,
};

const PAGE: SettingsPage = { page: "global", title: "General", sections: [LIMITS] };

const withPage = (child: ReactElement) =>
  createElement(
    SettingsContext,
    { value: { page: PAGE, project: null, edit: () => undefined, act: () => undefined } },
    child,
  );

describe("the Settings screen", () => {
  it("registers the overlay, its sections and ⌘,", () => {
    const app = registerScreens({ settings });
    expect(app.overlays.get("settings")).toBeDefined();
    expect(app.settings.global.ids()).toEqual([
      "agent_tools",
      "agents",
      "plugins",
      "agent_limits",
      "usage",
      "notifications",
      "data",
      "appearance",
      "keyboard_shortcuts",
      "tools",
      "about",
    ]);
    expect(app.settings.project.ids()).toContain("remove_project");
    expect(app.settings.project.ids().slice(0, 3)).toEqual(["general", "flow", "project_notes"]);
    expect(app.settings.project.ids()).not.toContain("validation");
    expect(app.settings.project.ids()).toContain("plugins");
    expect(app.shortcuts.run({ action: "settings" })).toBe(true);
    expect(overlays.store.get()).toEqual([{ id: "settings", params: undefined }]);
  });

  it("draws a section's rows from the Rust view: dimmed rows, a disabled stepper, a switch", () => {
    const out = html(withPage(createElement(Section, { section: LIMITS })));
    expect(out).toContain('<h3 class="settings-section-title">Agent limits</h3>');
    expect(out).toContain("Optional caps on how many agents run at once.");
    expect(out).toContain('role="switch"');
    expect(out).toContain('aria-checked="false"');
    expect(out).toContain('data-dimmed="true"');
    expect(out).toMatch(/<button type="button" aria-label="−" disabled="">/);
  });

  it("a registered section draws only when the page has it", () => {
    expect(html(withPage(createElement(sectionComponent("agent_limits"))))).toContain("Limit agents");
    expect(html(withPage(createElement(sectionComponent("tools"))))).toBe("");
    expect(sectionOf(PAGE, "agent_limits")?.rows).toHaveLength(2);
  });

  it("a flagged value shows its reason under the field", () => {
    const section: SettingsSection<"commands"> = {
      id: "commands",
      title: null,
      intro: null,
      rows: [
        {
          field: "open_url",
          label: "Open URL",
          description: "",
          control: {
            kind: "text",
            value: "localhost",
            placeholder: "none",
            mono: true,
            multiline: false,
            problem: "localhost isn't a URL",
          },
          dimmed: false,
          reset: null,
          button: null,
        },
      ],
      note: null,
    };
    const out = html(withPage(createElement(Section, { section })));
    expect(out).toContain('aria-invalid="true"');
    expect(out).toContain('<span class="settings-problem">localhost isn&#x27;t a URL</span>');
  });

  it("Ticket resources come right after Commands, and schema paths are one multi-line field", () => {
    const app = registerScreens({ settings });
    const ids = app.settings.project.ids();
    expect(ids.indexOf("ticket_resources")).toBe(ids.indexOf("commands") + 1);
    const section: SettingsSection<"ticket_resources"> = {
      id: "ticket_resources",
      title: "Ticket resources",
      intro: null,
      rows: [
        {
          field: "schema_paths",
          label: "Schema paths",
          description: "One path per line, like db/migrations/**.",
          control: {
            kind: "text",
            value: "db/migrations/**",
            placeholder: "none",
            mono: true,
            multiline: true,
            problem: "../x must be a path inside the repository",
          },
          dimmed: false,
          reset: null,
          button: null,
        },
      ],
      note: null,
    };
    const out = html(withPage(createElement(Section, { section })));
    expect(out).toContain('<h3 class="settings-section-title">Ticket resources</h3>');
    expect(out).toContain("<textarea");
    expect(out).toContain("db/migrations/**");
    expect(out).toContain('aria-invalid="true"');
  });

  it("opens the topic that holds a section, else the first", () => {
    const row = (label: string, sections: GlobalSectionId[]): SettingsNavRow => ({
      label,
      icon: "bot",
      target: { page: "global", section: sections[0] ?? "about", sections },
      alert: false,
    });
    const rows = [row("Agent tools", ["agent_tools"]), row("Agents", ["agents", "agent_limits"])];
    expect(topicRow(rows, "agent_limits")?.label).toBe("Agents");
    expect(topicRow(rows, "agents")?.label).toBe("Agents");
    expect(topicRow(rows, null)?.label).toBe("Agent tools");
    expect(topicRow([], null)).toBeUndefined();
  });
});

// Rows in the shapes `agent_tools_page.rs` sends for Settings (*15* `bGHVl`).
const toolRow = (tool: string, more: Partial<AgentToolRow>): AgentToolRow => ({
  tool,
  avatar: { tool: "codex", letters: "CX", background: "tool-cx", foreground: "tool-cx-fg", hover: "Codex" },
  name: "Codex",
  pill: null,
  detail: "",
  sign_in: null,
  show: null,
  install: null,
  install_button: null,
  install_note: null,
  update: null,
  can_see: null,
  notes: [],
  added: null,
  setup_failed: null,
  warning: null,
  menu: [],
  ...more,
});
const HIDE = { action: "hide", label: "Hide from Otaq", destructive: false } as const;
const RECHECK = { action: "recheck", label: "Re-check this tool", destructive: false } as const;
const SHOW = { action: "show", label: "Show", destructive: false } as const;
const DISCONNECT = { action: "disconnect", label: "Disconnect…", destructive: true } as const;
const INSTALL = { label: "How to install (macOS)", command: "brew install codex", copy: "Copy" };

const ROWS = {
  connected: toolRow("codex", {
    pill: { label: "Connected", look: "connected" },
    detail: "v0.157.1 · signed in with ChatGPT",
    can_see: {
      label: "Otaq can see",
      chips: [
        { label: "permission prompts", seen: true },
        { label: "limits not reported", seen: false },
      ],
    },
    notes: ["Limits come from Codex’s session file."],
    added: {
      text: "Otaq added: trust for your projects, its hooks, otaq pre-approved, its skills",
      stays: "Trust for your projects stays when you disconnect.",
    },
    setup_failed: "Otaq couldn’t add its skills (the plugin folder couldn’t be written).",
    warning: {
      title: "Your Codex settings ask for permission prompts",
      text: "With 5–10 agents…",
      link: null,
    },
    menu: [HIDE, RECHECK, DISCONNECT],
  }),
  signed_out: toolRow("copilot", {
    pill: { label: "Found · not signed in", look: "not_signed_in" },
    detail: "v1.0.2 · not signed in",
    sign_in: "Sign in…",
    menu: [HIDE, RECHECK],
  }),
  update: toolRow("copilot", {
    pill: { label: "Found · update needed", look: "update_needed" },
    detail: "v0.0.404 · too old for this app",
    update: {
      label: "Update GitHub Copilot to a version Otaq runs",
      command: "npm install -g @github/copilot",
      copy: "Copy",
    },
    menu: [HIDE, RECHECK],
  }),
  missing: toolRow("codex", {
    pill: { label: "Not installed", look: "not_installed" },
    detail: "not installed",
    install: INSTALL,
    menu: [RECHECK],
  }),
  hidden: toolRow("opencode", {
    pill: { label: "Hidden", look: "hidden" },
    detail: "Found, hidden by you",
    show: "Show",
    menu: [SHOW, RECHECK],
  }),
};

const toolsPage = (rows: AgentToolRow[]): AgentToolsPage => ({
  title: "Agent tools",
  intro: "The AI tools already on your machine.",
  rows,
  recheck: "Re-check",
  checked: "Checked 2 minutes ago",
  menu_label: "More actions",
  not_shown_title: "Not shown here",
  not_shown: "git, gh and az stay under Tools: they aren’t agents.",
  error: null,
  terminal: {
    hide: "Hide",
    close: "Close",
    retry: "Retry",
    checking: "Ended \u00b7 checking again\u2026",
    not_done: "Not done yet \u2014 see the output above",
    done: "Done",
  },
});

const drawRow = (row: AgentToolRow) =>
  html(
    createElement(AgentToolsPageBody, {
      page: toolsPage([row]),
      onAction: () => undefined,
      onRecheck: () => undefined,
    }),
  );

describe("Settings → Agent tools", () => {
  it("each state shows its pill and its own actions", () => {
    const connected = drawRow(ROWS.connected);
    expect(connected).toContain('data-look="connected"');
    expect(connected).toContain("v0.157.1 · signed in with ChatGPT");
    expect(connected).toContain("permission prompts");
    expect(connected).toContain("limits not reported");
    expect(connected).toContain("Otaq added: trust for your projects");
    expect(connected).toContain("Trust for your projects stays when you disconnect.");
    expect(connected).toContain('class="agent-tool-failed" role="alert"');
    expect(connected).toContain('class="agent-tool-warning" role="note"');
    expect(connected).not.toContain("Sign in…");

    const signedOut = drawRow(ROWS.signed_out);
    expect(signedOut).toContain('data-look="not_signed_in"');
    expect(signedOut).toContain("Sign in…");

    const update = drawRow(ROWS.update);
    expect(update).toContain("Update GitHub Copilot to a version Otaq runs");
    expect(update).toContain("npm install -g @github/copilot");

    const missing = drawRow(ROWS.missing);
    expect(missing).toContain("How to install (macOS)");
    expect(missing).toContain("brew install codex");
    expect(missing).not.toContain("Otaq can see");

    const hidden = drawRow(ROWS.hidden);
    expect(hidden).toContain("Found, hidden by you · ");
    expect(hidden).toContain(">Show</button>");

    const page = drawRow(ROWS.connected);
    expect(page).toContain("Re-check");
    expect(page).toContain("Checked 2 minutes ago");
    expect(page).toContain("git, gh and az stay under Tools");
  });

  it("no state is drawn turquoise: the warning, a failed setup and the found states are amber", () => {
    const all = Object.values(ROWS).map(drawRow).join("");
    expect(all).not.toMatch(/needs-you|attention-bg|--st-needs/);
    expect(markOf(ROWS.signed_out)).toBe("attention");
    expect(markOf(ROWS.update)).toBe("attention");
    expect(markOf(ROWS.connected)).toBe("connected");
    expect(markOf(ROWS.missing)).toBe("missing");
    expect(markOf(ROWS.hidden)).toBe("quiet");
  });

  it("each action sends its request; Disconnect asks first", () => {
    expect(stepOf("codex", { kind: "sign_in" })).toEqual({ kind: "sign_in", params: { tool: "codex" } });
    expect(stepOf("opencode", { kind: "show" })).toEqual({
      kind: "page",
      method: "agent_tools.set_hidden",
      params: { tool: "opencode", hidden: false },
    });
    expect(stepOf("codex", { kind: "menu", action: "hide" })).toEqual({
      kind: "page",
      method: "agent_tools.set_hidden",
      params: { tool: "codex", hidden: true },
    });
    expect(stepOf("codex", { kind: "menu", action: "recheck" })).toEqual({
      kind: "page",
      method: "agent_tools.recheck",
      params: { tool: "codex" },
    });
    // Never `agent_tools.disconnect` straight from the menu: the dialog comes first.
    expect(stepOf("codex", { kind: "menu", action: "disconnect" })).toEqual({
      kind: "ask_disconnect",
      params: { tool: "codex" },
    });
    expect(stepOf("claude", { kind: "docs" })).toEqual({ kind: "docs", params: { tool: "claude" } });
    const menu = agentToolMenu(ROWS.connected.menu);
    expect(menu.map((m) => m.id)).toEqual(["hide", "recheck", "disconnect"]);
    expect(menu[2]).toMatchObject({ danger: true, separated: true });
    expect(agentToolMenu(ROWS.hidden.menu).map((m) => m.id)).toEqual(["show", "recheck"]);
  });

  it("Disconnect's dialog: what it removes, what stays, and its refusal turns Disconnect off", () => {
    const dialog = {
      tool: "codex",
      title: "Disconnect Codex?",
      text: "Removes only what Otaq added (its hooks, otaq pre-approved, its skills).",
      stays: "Otaq added trust for your projects; it stays when you disconnect.",
      hidden: "Codex is then hidden: it doesn’t count until you show it again.",
      cancel: "Cancel",
      confirm: "Disconnect",
      error: null,
      can_confirm: true,
    };
    const props = { onCancel: () => undefined, onConfirm: () => undefined };
    const asks = html(createElement(DisconnectDialogView, { dialog, ...props }));
    expect(asks).toContain("Disconnect Codex?");
    expect(asks).toContain("it stays when you disconnect.");
    expect(asks).not.toMatch(/<button[^>]*disabled=""[^>]*>.*Disconnect<\/span>/);
    const refused = html(
      createElement(DisconnectDialogView, {
        dialog: { ...dialog, error: "Codex has 2 agents running. End them first.", can_confirm: false },
        ...props,
      }),
    );
    expect(refused).toContain("Codex has 2 agents running. End them first.");
    expect(refused).toMatch(/<button[^>]*disabled=""[^>]*>/);
  });
});
