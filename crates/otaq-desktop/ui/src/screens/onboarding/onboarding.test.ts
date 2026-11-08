import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { overlays } from "../../app/overlays";
import { registerScreens } from "../../app/registry";
import type { AddProjectView } from "../../generated/AddProjectView";
import type { AgentToolRow } from "../../generated/AgentToolRow";
import type { OnboardingTools } from "../../generated/OnboardingTools";
import type { ServiceConnection } from "../../generated/ServiceConnection";
import { droppedFolder, folderDropped } from "./drops";
import { onboarding, openOnboardingWhenNeeded } from ".";
import {
  added,
  adoptKey,
  afterTools,
  baseLabel,
  baseText,
  flowPicked,
  keyText,
  newForm,
  pickFlow,
  sameFlowPick,
  typeBase,
  typeKey,
} from "./logic";
import { AddButton, AddProjectFields, staticAddProject } from "./AddProjectForm";
import { ToolsList } from "./Tools";

afterEach(() => {
  overlays.store.set([]);
  droppedFolder.set(null);
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const TOOLS: OnboardingTools = {
  title: "Tools",
  text: "Otaq runs the coding agents you already use.",
  agent_tools: {
    heading: "AGENT TOOLS",
    count: "none ready",
    rows: [],
    blocked: null,
    menu_label: "More actions",
  },
  other_heading: "ALSO NEEDED",
  rows: [
    {
      tool: "git",
      name: "claude",
      badge: "required",
      look: "missing_required",
      icon: "circle-x",
      status: "not found on PATH",
      explanation: "nothing can start without it.",
      command: "npm install -g @anthropic-ai/claude-code",
      button: null,
      note: null,
      identity: null,
    },
  ],
  waiting: null,
  error: null,
  recheck: "Check again",
  continue_label: "Continue",
  continue_enabled: false,
  continue_reason: "Install claude to continue",
  has_projects: false,
  copy: "Copy",
  terminal: {
    hide: "Hide",
    close: "Close",
    retry: "Retry",
    checking: "Ended \u00b7 checking again\u2026",
    not_done: "Not done yet \u2014 see the output above",
    done: "Done",
  },
  agents_line: null,
};

describe("the onboarding screen", () => {
  it("registers onboarding and Add project", () => {
    const app = registerScreens({ onboarding });
    expect(app.overlays.ids().toSorted()).toEqual(["add_project", "onboarding"]);
  });

  it("opens once, after the first connection, when no project is stored", async () => {
    const asked: string[] = [];
    let push: (c: ServiceConnection) => void = () => undefined;
    const watch = (onChange: (c: ServiceConnection) => void) => {
      push = onChange;
      return () => undefined;
    };
    openOnboardingWhenNeeded((method) => {
      asked.push(method);
      return Promise.resolve({ show: true });
    }, watch);
    push({ state: "reconnecting", attempt: 1 });
    expect(asked).toEqual([]);
    const connected = {
      state: "connected",
      build_id: "x",
      compatibility: "same",
      agents_working: 0,
      agents_waiting: 0,
    } as ServiceConnection;
    push(connected);
    push(connected);
    await flush();
    expect(asked).toEqual(["onboarding.start"]);
    expect(overlays.store.get()).toEqual([{ id: "onboarding", params: undefined }]);
  });

  it("stays closed when a project exists", async () => {
    let push: (c: ServiceConnection) => void = () => undefined;
    openOnboardingWhenNeeded(
      () => Promise.resolve({ show: false }),
      (onChange) => {
        push = onChange;
        return () => undefined;
      },
    );
    push({ state: "connected" } as ServiceConnection);
    await flush();
    expect(overlays.store.get()).toEqual([]);
  });

  it("a dropped folder opens Add project, or goes to onboarding's project step", () => {
    folderDropped("/code/shop-web");
    expect(overlays.store.get()).toEqual([{ id: "add_project", params: { folder: "/code/shop-web" } }]);
    overlays.store.set([{ id: "onboarding", params: undefined }]);
    folderDropped("/code/billing-api");
    folderDropped("/code/billing-api");
    expect(droppedFolder.get()).toEqual({ path: "/code/billing-api", seq: 2 });
    expect(overlays.store.get().map((o) => o.id)).toEqual(["onboarding"]);
  });
});

describe("the steps", () => {
  it("Continue goes to the project step, or ends for a user with projects", () => {
    expect(afterTools(TOOLS)).toBe("project");
    expect(afterTools({ ...TOOLS, has_projects: true })).toBe("done");
  });

  it("a project added goes to its board; Suggest one opens Edit flow while the agent drafts", async () => {
    const went: string[] = [];
    const routes = {
      select: (p: string) => went.push(`select ${p}`),
      editFlow: (p: string) => went.push(`edit flow ${p}`),
      suggest: (p: string) => {
        went.push(`suggest ${p}`);
        return Promise.resolve();
      },
    };
    added("SHOP", undefined, routes);
    expect(went).toEqual(["select SHOP"]);
    went.length = 0;
    added("BILL", { kind: "same_as", project: "SHOP" }, routes);
    expect(went).toEqual(["select BILL"]);
    went.length = 0;
    added("BILL", { kind: "suggest" }, routes);
    await flush();
    expect(went).toEqual(["select BILL", "edit flow BILL", "suggest BILL"]);
  });

  it("the Flow choice starts on Otaq default and keeps the pick", () => {
    const form = newForm(false);
    expect(flowPicked(form)).toEqual({ kind: "default" });
    const same = pickFlow(form, { kind: "same_as", project: "SHOP" });
    expect(sameFlowPick(flowPicked(same), { kind: "same_as", project: "SHOP" })).toBe(true);
    expect(sameFlowPick(flowPicked(same), { kind: "same_as", project: "BILL" })).toBe(false);
    expect(sameFlowPick(flowPicked(same), { kind: "default" })).toBe(false);
  });

  it("a missing claude blocks Continue, with the reason and the copyable command", () => {
    const html = renderToStaticMarkup(
      createElement(ToolsList, {
        tools: TOOLS,
        checking: false,
        onRecheck: () => undefined,
        onContinue: () => undefined,
        onAgentAction: () => undefined,
      }),
    );
    expect(html).toContain("Install claude to continue");
    expect(html).toContain("npm install -g @anthropic-ai/claude-code");
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>.*Continue/);
    expect(html).toContain('data-look="missing_required"');
  });
});

// An agent tool's row as `agent_tools_page::row` sends it for the Tools step (no pill, no Disconnect…).
const agentRow = (tool: string, name: string, more: Partial<AgentToolRow>): AgentToolRow => ({
  tool,
  avatar: { tool: "codex", letters: "CX", background: "tool-cx", foreground: "tool-cx-fg", hover: name },
  name,
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
  menu: [{ action: "recheck", label: "Re-check this tool", destructive: false }],
  ...more,
});

describe("the agent tools on the Tools step", () => {
  const render = (tools: OnboardingTools) =>
    renderToStaticMarkup(
      createElement(ToolsList, {
        tools,
        checking: false,
        onRecheck: () => undefined,
        onContinue: () => undefined,
        onAgentAction: () => undefined,
      }),
    );

  it("none connected: the blocking line, each install command, and Continue off with its reason", () => {
    const install = {
      label: "How to install (macOS)",
      command: "brew install codex",
      copy: "Copy",
    };
    const html = render({
      ...TOOLS,
      rows: [],
      continue_reason: "Connect one agent tool to continue",
      agent_tools: {
        ...TOOLS.agent_tools,
        blocked:
          "No agent tool is connected. Install and sign in to at least one; agents can’t start without it.",
        rows: [agentRow("codex", "Codex", { detail: "not installed", install })],
      },
    });
    expect(html).toContain('class="agent-tools-blocked" role="alert"');
    expect(html).toContain("none ready");
    expect(html).toContain('data-mark="missing"');
    expect(html).toContain("brew install codex");
    expect(html).toContain("Connect one agent tool to continue");
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>.*Continue/);
    expect(html).toContain("ALSO NEEDED");
  });

  it("one connected: Continue on; a signed-out tool offers Sign in…, a hidden one Show", () => {
    const html = render({
      ...TOOLS,
      rows: [],
      continue_enabled: true,
      continue_reason: null,
      agent_tools: {
        ...TOOLS.agent_tools,
        count: "1 ready · 1 hidden",
        rows: [
          agentRow("claude", "Claude Code", {
            detail: "v2.1.301 · signed in",
            can_see: { label: "Otaq can see", chips: [{ label: "prompts", seen: true }] },
          }),
          agentRow("copilot", "GitHub Copilot", { detail: "found · not signed in", sign_in: "Sign in…" }),
          agentRow("opencode", "OpenCode", { detail: "hidden by you", show: "Show" }),
        ],
      },
    });
    expect(html).not.toMatch(/<button[^>]*disabled=""[^>]*>.*Continue/);
    expect(html).toContain("1 ready · 1 hidden");
    expect(html.match(/data-mark="([a-z]+)"/g)).toEqual([
      'data-mark="connected"',
      'data-mark="attention"',
      'data-mark="quiet"',
    ]);
    expect(html).toContain("Sign in…");
    expect(html).toContain("hidden by you · ");
    expect(html).toContain(">Show</button>");
    expect(html).not.toContain("Disconnect");
  });
});

describe("the Add project form", () => {
  const view = (key: string, base = "main") =>
    ({ key: { value: key }, base: { value: base } }) as unknown as AddProjectView;

  it("shows Rust's suggestion until the user types, then what they typed", () => {
    const form = newForm(true, "/code/shop-web");
    expect(form).toEqual({
      folder: "/code/shop-web",
      key: "",
      key_edited: false,
      base_branch: "",
      base_edited: false,
      first_run: true,
    });
    expect(keyText(form, view("SHOP"))).toBe("SHOP");
    const typed = typeKey(form, "sh");
    expect(keyText(typed, view("SHOP"))).toBe("sh");
    expect(baseText(form, view("SHOP", "develop"))).toBe("develop");
    expect(baseText(typeBase(form, ""), view("SHOP", "develop"))).toBe("");
  });

  it("shows a picked remote branch by its short name", () => {
    const row = (id: string, label: string) => ({ id, label, header: false });
    const withItems = {
      key: { value: "SHOP" },
      base: { value: "main", items: [row("main", "main"), row("origin/next", "next")] },
    } as unknown as AddProjectView;
    const form = newForm(true, "/code/shop-web");
    expect(baseLabel(form, withItems)).toBe("main");
    expect(baseLabel(typeBase(form, "origin/next"), withItems)).toBe("next");
    expect(baseLabel(typeBase(form, "gone"), withItems)).toBe("gone");
  });

  it("takes Rust's spelling of the key, unless the user typed on meanwhile", () => {
    const asked = typeKey(newForm(false), "shop");
    expect(adoptKey(asked, asked, view("SHOP")).key).toBe("SHOP");
    const later = typeKey(asked, "shopx");
    expect(adoptKey(later, asked, view("SHOP"))).toBe(later);
    const untouched = newForm(false);
    expect(adoptKey(untouched, untouched, view("SHOP"))).toBe(untouched);
  });
});

describe("the folder trust in the Add project form", () => {
  const LINE = "Claude Code will run its hooks and settings in this folder.";
  const FORM: AddProjectView = {
    first_run: false,
    title: "Add a project",
    text: null,
    folder: "~/code/pocket-notes",
    folder_look: "valid",
    choose: "Change…",
    choose_quiet: true,
    picker_title: "Add project",
    error: null,
    repo_path: "/code/pocket-notes",
    name: null,
    base: null,
    key: null,
    footnote: null,
    add: "Add project",
    adding: "Adding…",
    can_add: true,
    add_error: null,
    cancel: "Cancel",
    trust_line: LINE,
    trust_error: null,
    continue_label: "Continue",
    flow: null,
  };
  const draw = (view: AddProjectView) => {
    const state = staticAddProject(view);
    return renderToStaticMarkup(
      createElement(
        "div",
        null,
        createElement(AddProjectFields, { state }),
        createElement(AddButton, { state }),
      ),
    );
  };

  it("shows the line under the folder, muted, and Add", () => {
    const html = draw(FORM);
    expect(html).toContain('<span class="add-project-trust"><span class="icon"');
    expect(html).toContain(`<span>${LINE}</span>`);
    expect(html).not.toContain('role="alert"');
    expect(html).toContain("Add project");
    expect(draw({ ...FORM, trust_line: null })).not.toContain("add-project-trust");
  });

  it("after a failed trust: the reason in the inline error style, and Continue instead of Add", () => {
    const html = draw({
      ...FORM,
      can_add: false,
      trust_error:
        "Claude Code’s trust couldn’t be recorded: why. Its agents will wait on Claude Code’s trust screen.",
    });
    expect(html).toMatch(
      /<span class="add-project-failure" role="alert">Claude Code’s trust couldn’t be recorded/,
    );
    expect(html).toMatch(/<button[^>]*>.*Continue/);
    expect(html).not.toMatch(/disabled=""[^>]*>.*Continue/);
    expect(html).not.toContain("Adding…");
  });
});
