// @vitest-environment happy-dom
// App settings is about you and your machine: eight topics in the nav, no projects, a line saying where project
// settings are; Settings opens at a topic's id (`git`, which the PR dialog and the project's Repository card link to);
// Git & pull requests shows git and one row per PR provider with its sign-in state.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { RegistriesContext, registerScreens } from "../../app/registry";
import type { GitSettingsView } from "../../generated/GitSettingsView";
import type { GlobalSectionId } from "../../generated/GlobalSectionId";
import type { SettingsNavRow } from "../../generated/SettingsNavRow";
import type { SettingsView } from "../../generated/SettingsView";
import { request } from "../../tauri";
import { GitBody } from "./Git";
import { Settings } from "./Settings";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

vi.mock("../../service", async (original) => ({
  ...(await original<typeof import("../../service")>()),
  watchView: (load: () => Promise<unknown>, _on: unknown, onChange: (state: { view?: unknown }) => void) => {
    void load().then((view) => {
      onChange({ view });
    });
    return () => undefined;
  },
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const topic = (label: string, sections: GlobalSectionId[]): SettingsNavRow => ({
  label,
  icon: "bot",
  target: { page: "global", section: sections[sections.length - 1] ?? "about", sections },
});

// The nav `settings_page.rs` sends.
const NAV: SettingsNavRow[] = [
  topic("Agents", ["agent_tools", "agents"]),
  topic("Git & pull requests", ["git"]),
  topic("Limits & usage", ["agent_limits", "usage"]),
  topic("Notifications", ["notifications"]),
  topic("Appearance", ["appearance"]),
  topic("Keyboard", ["keyboard_shortcuts"]),
  topic("Data", ["data"]),
  topic("About", ["about"]),
];

const VIEW: SettingsView = {
  title: "Settings",
  footnote: "Project settings live in each project’s Settings tab.",
  close_label: "Close settings",
  nav: NAV,
  page: {
    page: "global",
    title: "General",
    sections: NAV.flatMap((row) => row.target.sections).map((id) => ({
      id,
      title: null,
      intro: null,
      rows: [],
      note: null,
    })),
  },
};

const marker = (id: string) => () => createElement("p", { className: "marker" }, id);

const app = registerScreens({
  settings: (a) => {
    a.overlays.register("settings", { Component: Settings });
    for (const row of NAV)
      for (const id of row.target.sections) a.settings.global.register(id, { Component: marker(id) });
  },
});

let root: Root;
let host: HTMLElement;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.mocked(request).mockImplementation((method: string) =>
    method === "settings.view" ? Promise.resolve(VIEW) : Promise.reject(new Error(method)),
  );
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
});

async function open(params?: { section?: GlobalSectionId }) {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: app },
        createElement(Settings, { params, close: () => undefined }),
      ),
    );
  });
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

const texts = (selector: string) => [...host.querySelectorAll(selector)].map((el) => el.textContent.trim());

describe("app settings", () => {
  it("lists the eight topics and no projects, with a line saying where project settings are", async () => {
    await open();
    expect(texts(".settings-nav-item")).toEqual([
      "Agents",
      "Git & pull requests",
      "Limits & usage",
      "Notifications",
      "Appearance",
      "Keyboard",
      "Data",
      "About",
    ]);
    expect(texts(".settings-nav-footnote")).toEqual([
      "Project settings live in each project’s Settings tab.",
    ]);
    // The first topic, Agents: its tools above its agents.
    expect(texts(".marker")).toEqual(["agent_tools", "agents"]);
  });

  it("opens at a topic's id: `git` shows Git & pull requests", async () => {
    await open({ section: "git" });
    expect(texts(".settings-header h2")).toEqual(["Git & pull requests"]);
    expect(texts(".marker")).toEqual(["git"]);
    expect(host.querySelector('.settings-nav-item[aria-current="true"]')?.textContent.trim()).toBe(
      "Git & pull requests",
    );
  });

  it("each topic opens with its own sections", async () => {
    await open();
    const shown: string[][] = [];
    for (const item of [...host.querySelectorAll<HTMLButtonElement>(".settings-nav-item")]) {
      act(() => {
        item.click();
      });
      shown.push(texts(".marker"));
    }
    expect(shown).toEqual([
      ["agent_tools", "agents"],
      ["git"],
      ["agent_limits", "usage"],
      ["notifications"],
      ["appearance"],
      ["keyboard_shortcuts"],
      ["data"],
      ["about"],
    ]);
  });
});

describe("Git & pull requests", () => {
  const view: GitSettingsView = {
    intro: "Installed once on this machine and used by every project.",
    git_title: "Git",
    git: {
      tool: "git",
      name: "git",
      detail: "2.46.0 · /usr/bin/git",
      state: "Needed by every project",
      look: "ready",
      command: null,
      button: null,
      note: null,
    },
    recheck_label: "Re-check",
    providers_title: "Pull request providers",
    providers: [
      {
        tool: "gh",
        name: "GitHub",
        detail: "uses gh 2.62.0",
        state: "Signed in as nt",
        look: "ready",
        command: null,
        button: null,
        note: null,
      },
      {
        tool: "az",
        name: "Azure DevOps",
        detail: "uses az 2.67.0",
        state: "Not signed in",
        look: "attention",
        command: "az login",
        button: { tool: "az", action: "login", label: "Sign in…", running: "Signing in to az…" },
        note: null,
      },
    ],
    footnote: "Otaq uses them to push and open pull requests; it never watches them.",
    error: null,
    copy: "Copy",
    terminal: {
      hide: "Hide",
      close: "Close",
      retry: "Retry",
      checking: "checking",
      not_done: "not done",
      done: "Done",
    },
  };

  it("shows git, then one row per provider with its tool and sign-in state, and Sign in… where it is needed", () => {
    const terminals = {
      open: null,
      failure: null,
      start: () => undefined,
      retry: () => undefined,
      hide: () => undefined,
      onPhase: () => undefined,
    };
    act(() => {
      root.render(createElement(GitBody, { view, checking: false, onRecheck: () => undefined, terminals }));
    });
    const rows = [...host.querySelectorAll(".settings-git-row")].map((row) =>
      [".settings-tool-name", ".settings-git-detail", ".settings-git-state"].map(
        (s) => row.querySelector(s)?.textContent ?? "",
      ),
    );
    expect(rows).toEqual([
      ["git", "2.46.0 · /usr/bin/git", "Needed by every project"],
      ["GitHub", "uses gh 2.62.0", "Signed in as nt"],
      ["Azure DevOps", "uses az 2.67.0", "Not signed in"],
    ]);
    expect(texts("button").filter((t) => t === "Sign in…")).toHaveLength(1);
    expect(texts("button")).toContain("Re-check");
  });
});
