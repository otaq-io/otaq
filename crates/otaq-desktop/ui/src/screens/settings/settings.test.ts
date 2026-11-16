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
import {
  type AgentToolAction,
  agentToolMenu,
  AgentToolsPageBody,
  DisconnectDialogView,
  effortEdit,
  markOf,
  noyanEdit,
  RecheckButton,
  stepOf,
} from "./AgentTools";

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
      set_by: null,
    },
    {
      field: "agent_limits.max_concurrency",
      label: "Max agents running at once",
      description: "Suggested 5. Started tickets wait for a free agent.",
      control: { kind: "stepper", value: 5, min: 1, max: 50, suffix: "", disabled: true, empty: null },
      dimmed: true,
      reset: null,
      button: null,
      set_by: null,
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
    // Eight topics: Tools, Git & pull requests, Limits & usage (the limit, then usage),
    // Notifications, Appearance, Keyboard, Data, About.
    expect(app.settings.global.ids()).toEqual([
      "tools",
      "git",
      "agent_limits",
      "usage",
      "notifications",
      "appearance",
      "keyboard_shortcuts",
      "data",
      "about",
    ]);
    // A project's settings are its own page: the overlay registers none of its cards.
    expect(app.settings.project.ids()).toEqual([]);
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
    expect(html(withPage(createElement(sectionComponent("notifications"))))).toBe("");
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
          set_by: null,
        },
      ],
      note: null,
    };
    const out = html(withPage(createElement(Section, { section })));
    expect(out).toContain('aria-invalid="true"');
    expect(out).toContain('<span class="settings-problem">localhost isn&#x27;t a URL</span>');
  });

  it("a run command an agent set says so, and Undo asks for it (agents run the app)", async () => {
    const section: SettingsSection<"commands"> = {
      id: "commands",
      title: null,
      intro: null,
      rows: [
        {
          field: "run_command",
          label: "Run command",
          description: "",
          control: {
            kind: "text",
            value: "node server.js",
            placeholder: "not set",
            mono: true,
            multiline: false,
            problem: null,
          },
          dimmed: false,
          reset: null,
          button: null,
          set_by: {
            text: "Set by Dev on SHOP-27",
            app: null,
            undo: "Undo",
            undo_hint: "Empties the run command the agent set",
          },
        },
      ],
      note: null,
    };
    expect(html(withPage(createElement(Section, { section })))).toMatch(
      /Run command.*Set by Dev on SHOP-27.*Undo/,
    );

    const acted: string[] = [];
    const host = document.body.appendChild(document.createElement("div"));
    const { createRoot } = await import("react-dom/client");
    const { act } = await import("react");
    (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const root = createRoot(host);
    act(() => {
      root.render(
        createElement(
          SettingsContext,
          {
            value: {
              page: PAGE,
              project: "SHOP",
              edit: () => undefined,
              act: (a) => {
                acted.push(a);
              },
            },
          },
          createElement(Section, { section }),
        ),
      );
    });
    const undo = [...host.querySelectorAll("button")].find((b) => b.textContent === "Undo");
    act(() => {
      undo?.click();
    });
    expect(acted).toEqual(["undo_run_command"]);
    act(() => {
      root.unmount();
    });
    host.remove();
  });

  it("schema paths are one multi-line field", () => {
    const section: SettingsSection<"workspace"> = {
      id: "workspace",
      title: "Workspace",
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
          set_by: null,
        },
      ],
      note: null,
    };
    const out = html(withPage(createElement(Section, { section })));
    expect(out).toContain('<h3 class="settings-section-title">Workspace</h3>');
    expect(out).toContain("<textarea");
    expect(out).toContain("db/migrations/**");
    expect(out).toContain('aria-invalid="true"');
  });

  it("opens the topic that holds a section, else the first", () => {
    const row = (label: string, sections: GlobalSectionId[]): SettingsNavRow => ({
      label,
      icon: "bot",
      target: { page: "global", section: sections[0] ?? "about", sections },
    });
    const rows = [row("Tools", ["tools"]), row("Limits", ["usage", "agent_limits"])];
    expect(topicRow(rows, "agent_limits")?.label).toBe("Limits");
    expect(topicRow(rows, "usage")?.label).toBe("Limits");
    expect(topicRow(rows, null)?.label).toBe("Tools");
    expect(topicRow([], null)).toBeUndefined();
  });
});

// Rows in the shapes `agent_tools_page.rs` sends for Settings (*15* `bGHVl`).
const toolRow = (tool: string, more: Partial<AgentToolRow>): AgentToolRow => ({
  tool,
  avatar: { tool: "codex", letters: "CX", background: "tool-cx", foreground: "tool-cx-fg", hover: "Codex" },
  name: "Codex",
  mark: "quiet",
  pill: null,
  detail: "",
  sign_in: null,
  recheck: null,
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
    mark: "connected",
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
    mark: "attention",
    pill: { label: "Found · not signed in", look: "not_signed_in" },
    detail: "v1.0.2 · not signed in",
    sign_in: "Sign in…",
    menu: [HIDE, RECHECK],
  }),
  could_not_check: toolRow("claude", {
    mark: "attention",
    pill: { label: "Found · couldn’t check", look: "could_not_check" },
    detail: "v2.1.319 · couldn’t check sign-in",
    recheck: "Re-check",
    menu: [HIDE, RECHECK],
  }),
  update: toolRow("copilot", {
    mark: "attention",
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
    mark: "missing",
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
  title: "Tools",
  intro: "The AI tools on this Mac.",
  rows_heading: "TOOLS",
  noyan: null,
  used: null,
  rows,
  recheck: "Re-check",
  checked: "Checked 2 minutes ago",
  menu_label: "More actions",
  not_shown_title: "Not shown here",
  not_shown: "git, gh and az are under Git & pull requests: they aren’t agents.",
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

const AVATAR = {
  tool: "claude_code",
  letters: "CC",
  background: "#000",
  foreground: "#fff",
  hover: "#111",
} as const;

// Noyan starts on and Where they're used, as `agent_tools_page.rs`'s `with_flows` sends them (canvas A-8).
const withFlows = (page: AgentToolsPage): AgentToolsPage => ({
  ...page,
  noyan: {
    heading: "NOYAN",
    label: "Noyan starts on",
    picker: {
      model_label: "Model",
      value: { avatar: AVATAR, text: "Claude Code · opus", no_longer_listed: null },
      menu: {
        title: "Pick a model…",
        pairs_heading: null,
        pairs: [],
        groups: [],
        missing: null,
        set_up: "Set up…",
        foot: "",
      },
      hint: "For new conversations. Switch for one conversation in the Noyan panel.",
      not_set_up: null,
      effort: {
        label: "Effort",
        levels: [
          { id: "", label: "Default", selected: false },
          { id: "medium", label: "Medium", selected: false },
          { id: "high", label: "High", selected: true },
        ],
        hint: "How hard the model thinks.",
      },
    },
  },
  used: {
    heading: "WHERE THEY’RE USED",
    rows: [
      {
        tool: "claude",
        avatar: AVATAR,
        name: "Claude Code",
        places: "shop-web: Dev · billing-api: Dev, Security review · Noyan",
        off: null,
      },
      {
        tool: "codex",
        avatar: { ...AVATAR, tool: "codex", letters: "CX" },
        name: "Codex",
        places: "shop-web: Validate",
        off: "Codex isn’t set up on this Mac: these wait until you set it up or pick another model.",
      },
    ],
    note: "To change one, open its project’s flow. Signing out of a tool makes its stages wait until you pick another model.",
  },
});

describe("Settings → Tools: Noyan starts on and where they're used", () => {
  it("draws Noyan's model and effort under the tools, then each tool's stages, read-only", () => {
    const drawn = html(
      createElement(AgentToolsPageBody, {
        page: withFlows(toolsPage([ROWS.connected])),
        onAction: () => undefined,
        onRecheck: () => undefined,
      }),
    );
    const at = (text: string) => drawn.indexOf(text);
    expect(at("TOOLS")).toBeGreaterThan(-1);
    expect(at("NOYAN")).toBeGreaterThan(at("TOOLS"));
    expect(drawn).toContain("Noyan starts on");
    expect(drawn).toContain("Claude Code · opus");
    expect(drawn).toContain("For new conversations. Switch for one conversation in the Noyan panel.");
    expect(at("WHERE THEY’RE USED")).toBeGreaterThan(at("NOYAN"));
    expect(drawn).toContain("shop-web: Dev · billing-api: Dev, Security review · Noyan");
    expect(drawn).toContain("Codex isn’t set up on this Mac");
    expect(drawn).toContain("To change one, open its project’s flow.");
    // Read-only: no button in Where they're used.
    const used = drawn.slice(at("WHERE THEY’RE USED"), at("Not shown here"));
    expect(used).not.toContain("<button");
  });

  it("a pick or an Effort level becomes agent_tools.noyan's edit", () => {
    expect(noyanEdit({ kind: "model", model: "codex" })).toEqual({ edit: "model", model: "codex" });
    expect(noyanEdit({ kind: "pair", model: "claude:opus", effort: "high" })).toEqual({
      edit: "pair",
      model: "claude:opus",
      effort: "high",
    });
    expect(noyanEdit({ kind: "pair", model: "opencode", effort: null })).toEqual({
      edit: "pair",
      model: "opencode",
    });
    expect(effortEdit(null)).toEqual({ edit: "effort" });
    expect(effortEdit("low")).toEqual({ edit: "effort", effort: "low" });
  });
});

describe("Settings → Tools", () => {
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

    // A tool whose own sign-in answer couldn't be read: never "signed in", Re-check on the row, no Sign in….
    const unchecked = drawRow(ROWS.could_not_check);
    expect(unchecked).toContain('data-look="could_not_check"');
    expect(unchecked).toContain("v2.1.319 · couldn’t check sign-in");
    expect(unchecked).not.toContain("signed in");
    expect(unchecked).not.toContain("Sign in…");
    // The row's own actions (the page's Re-check is under the list).
    const rowActions = (drawn: string) =>
      /agent-tool-actions">(.*?)<span class="agent-tool-more">/.exec(drawn)?.[1] ?? "";
    expect(rowActions(unchecked)).toContain("<span>Re-check</span></button>");
    expect(rowActions(signedOut)).not.toContain("Re-check");
    expect(rowActions(signedOut)).toContain("Sign in…");

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
    expect(page).toContain("git, gh and az are under Git &amp; pull requests");
  });

  it("no state is drawn turquoise: the warning, a failed setup and the found states are amber", () => {
    const all = Object.values(ROWS).map(drawRow).join("");
    expect(all).not.toMatch(/needs-you|attention-bg|--st-needs/);
    expect(markOf(ROWS.signed_out)).toBe("attention");
    expect(markOf(ROWS.update)).toBe("attention");
    expect(markOf(ROWS.could_not_check)).toBe("attention");
    expect(markOf(ROWS.connected)).toBe("connected");
    expect(markOf(ROWS.missing)).toBe("missing");
    expect(markOf(ROWS.hidden)).toBe("quiet");
  });

  it("a row's Re-check checks that tool again", () => {
    const actions: AgentToolAction[] = [];
    const button = RecheckButton({
      row: ROWS.could_not_check,
      onAction: (action) => actions.push(action),
    }) as ReactElement<{ label: string; onClick: () => void }>;
    expect(button.props.label).toBe("Re-check");
    button.props.onClick();
    expect(actions).toEqual([{ kind: "menu", action: "recheck" }]);
    expect(stepOf("claude", { kind: "menu", action: "recheck" })).toEqual({
      kind: "page",
      method: "agent_tools.recheck",
      params: { tool: "claude" },
    });
    expect(RecheckButton({ row: ROWS.signed_out, onAction: () => undefined })).toBeNull();
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
