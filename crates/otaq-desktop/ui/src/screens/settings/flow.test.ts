// @vitest-environment happy-dom
// Settings → Flow (T-0422; FLW-06…FLW-11): the section draws Rust's table and waiting line; every action but Export
// puts its flow in the draft, closes Settings and opens the flow editor on it (FED-09); Export and Import go through
// the system's dialogs and a cancelled dialog does nothing; Change flow… opens the editor and starts nothing; a
// preset card proposes its file. (The flow banner's Apply now and Cancel change: `notices/applyNow.test.ts`.)
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, type ShellLayout, shellLayout } from "../../app/layout";
import { NOTHING_SELECTED, selection } from "../../app/selection";
import { createStore } from "../../app/store";
import type { FlowSettings } from "../../generated/FlowSettings";
import type { MenuItem } from "../../generated/MenuItem";
import type { PresetDialog as PresetDialogView } from "../../generated/PresetDialog";
import { request } from "../../tauri";
import { type AgentPanelFocusRequest, agentPanelFocusRequests, shownAgents } from "../agent_panel/logic";
import { FlowSectionBody, PresetDialog } from "./Flow";
import { changeFlow, exportFlow, type FlowRoutes, importFlow, menuChoice, proposeFrom } from "./flowActions";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  pickFolder: vi.fn(),
  pickFlowFile: vi.fn(),
  saveFlowFile: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
  windowAction: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const item = (id: string, label: string, over: Partial<MenuItem> = {}): MenuItem => ({
  id,
  label,
  icon: null,
  shortcut: null,
  danger: false,
  disabled: false,
  hint: null,
  separated: false,
  checked: false,
  header: false,
  ...over,
});

// The shape `flow_settings.rs` sends for the test flow (its view-model test), waiting for a quiet board.
const SECTION: FlowSettings = {
  project: "SHOP",
  title: "Flow",
  changed: "Changed by Noyan · yesterday",
  intro:
    "The project’s columns and who works in each. Change it in the flow editor; nothing reaches the board until you apply.",
  waiting: {
    text: "A flow change is waiting: QA after Security. 8 tickets are still working, and no new ticket starts until they’re done.",
    cancel_label: "Cancel change",
  },
  columns: ["Stage", "Agent · model", "Options a ticket can pick"],
  ends: "Backlog → Ready for Dev → … → Done: always first and last",
  rows: [
    {
      icon: "bot",
      name: "Dev",
      kind: "Agent",
      agent: "Rust developer",
      model: "Claude Code · sonnet",
      options: "Quick developer",
    },
    { icon: "user-check", name: "Review", kind: "Check · you", agent: "You", model: null, options: "skip" },
  ],
  problem: null,
  change: { label: "Change flow…", icon: "workflow", disabled: false, hint: null },
  preset: { label: "Use a preset…", icon: null, disabled: false, hint: null },
  copy: { label: "Copy from another project…", icon: null, disabled: false, hint: null },
  menu_label: "More flow actions",
  menu: [
    item("import", "Import flow…"),
    item("export", "Export flow…"),
    item("back", "Back to the previous flow", { separated: true }),
    item("reset", "Reset to the default flow"),
  ],
  export_title: "Export shop-web’s flow",
  export_file: "shop-web-flow.yaml",
  import_title: "Import a flow",
};

const ask = vi.mocked(request);
let box: HTMLElement;
let root: Root;

function routes(over: Partial<FlowRoutes> = {}): FlowRoutes & { closed: () => number; editors: string[] } {
  let closed = 0;
  const editors: string[] = [];
  return {
    ask,
    closeSettings: () => {
      closed += 1;
    },
    openEditor: (project) => {
      editors.push(project);
    },
    editors,
    pickFile: vi.fn(() => Promise.resolve(null)),
    saveFile: vi.fn(() => Promise.resolve(null)),
    agentPanel: {
      selection: createStore(NOTHING_SELECTED),
      layout: createStore<ShellLayout>({ ...INITIAL_LAYOUT, agentOpen: false }),
      shown: createStore<Record<string, string | null>>({}),
      focus: createStore<AgentPanelFocusRequest | null>(null),
    },
    closed: () => closed,
    ...over,
  };
}

beforeEach(() => {
  ask.mockReset();
  ask.mockResolvedValue(null);
  box = document.body.appendChild(document.createElement("div"));
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
  shownAgents.set({});
  agentPanelFocusRequests.set(null);
  shellLayout.set(INITIAL_LAYOUT);
  selection.set(NOTHING_SELECTED);
});

describe("the Flow section", () => {
  it("draws who changed it, the waiting line, the table and the actions", () => {
    const cancel = vi.fn();
    act(() => {
      root.render(
        createElement(FlowSectionBody, {
          section: SECTION,
          onCancelChange: cancel,
          onChange: vi.fn(),
          onPreset: vi.fn(),
          onCopy: vi.fn(),
          onMenu: vi.fn(),
        }),
      );
    });
    const text = box.textContent;
    for (const words of [
      "Changed by Noyan · yesterday",
      "A flow change is waiting: QA after Security.",
      "Backlog → Ready for Dev → … → Done: always first and last",
      "Rust developer",
      "Claude Code · sonnet",
      "Quick developer",
      "Check · you",
      "Change flow…",
      "Use a preset…",
      "Copy from another project…",
    ])
      expect(text).toContain(words);
    expect(box.querySelectorAll(".settings-flow-row:not(.settings-flow-head)")).toHaveLength(2);
    const button = [...box.querySelectorAll("button")].find((b) => b.textContent === "Cancel change");
    act(() => {
      button?.click();
    });
    expect(cancel).toHaveBeenCalledOnce();
  });

  it("opens the ⋯ menu with Import, Export, Back and Reset", () => {
    const onMenu = vi.fn();
    act(() => {
      root.render(
        createElement(FlowSectionBody, {
          section: SECTION,
          onCancelChange: vi.fn(),
          onChange: vi.fn(),
          onPreset: vi.fn(),
          onCopy: vi.fn(),
          onMenu,
        }),
      );
    });
    const more = box.querySelector<HTMLButtonElement>('button[aria-label="More flow actions"]');
    act(() => {
      more?.click();
    });
    const rows = [...document.querySelectorAll('[role="menuitem"]')].map((r) => r.textContent);
    expect(rows).toEqual([
      "Import flow…",
      "Export flow…",
      "Back to the previous flow",
      "Reset to the default flow",
    ]);
  });

  it("a preset card proposes its file", () => {
    const dialog: PresetDialogView = {
      project: "SHOP",
      title: "Use a preset for shop-web",
      cancel: "Cancel",
      presets: [
        {
          name: "Grill first",
          delivers: "Done · click",
          line: "Settle the ticket with you before any code is written",
          columns: [
            { icon: "hourglass", label: "Ready for Dev" },
            { icon: "bot", label: "Grill" },
            { icon: "git-merge", label: "Done" },
          ],
          file: "grill-first.yaml",
        },
      ],
    };
    const onPick = vi.fn();
    act(() => {
      root.render(createElement(PresetDialog, { dialog, onPick, onCancel: vi.fn() }));
    });
    expect(document.body.textContent).toContain("Use a preset for shop-web");
    expect(document.body.textContent).toContain("grill-first.yaml");
    act(() => {
      document.querySelector<HTMLButtonElement>(".settings-preset")?.click();
    });
    expect(onPick).toHaveBeenCalledWith("grill-first.yaml");
  });
});

describe("the Flow actions", () => {
  it("an action's flow lands in the draft and the editor opens on it; a refusal keeps Settings open", async () => {
    const r = routes();
    await proposeFrom("SHOP", { from: "preset", file: "grill-first.yaml" }, r);
    expect(ask).toHaveBeenCalledWith("settings.flow_propose", {
      project: "SHOP",
      source: { from: "preset", file: "grill-first.yaml" },
    });
    expect(r.closed()).toBe(1);
    expect(r.editors).toEqual(["SHOP"]);
    ask.mockRejectedValueOnce({ message: "SHOP has no previous flow" });
    await expect(menuChoice(SECTION, "back", r)).rejects.toBeTruthy();
    expect(ask).toHaveBeenLastCalledWith("settings.flow_propose", {
      project: "SHOP",
      source: { from: "previous" },
    });
    expect(r.closed()).toBe(1);
    await menuChoice(SECTION, "reset", r);
    expect(ask).toHaveBeenLastCalledWith("settings.flow_propose", {
      project: "SHOP",
      source: { from: "default" },
    });
  });

  it("Import reads the picked file into the draft; a cancelled dialog asks nothing", async () => {
    const r = routes();
    await importFlow(SECTION, r);
    expect(ask).not.toHaveBeenCalled();
    const picked = routes({ pickFile: vi.fn(() => Promise.resolve("/tmp/flow.yaml")) });
    await importFlow(SECTION, picked);
    expect(picked.pickFile).toHaveBeenCalledWith("Import a flow");
    expect(ask).toHaveBeenCalledWith("settings.flow_import", { project: "SHOP", path: "/tmp/flow.yaml" });
    expect(picked.closed()).toBe(1);
    expect(picked.editors).toEqual(["SHOP"]);
  });

  it("Export saves where the dialog said and keeps Settings open", async () => {
    const r = routes({ saveFile: vi.fn(() => Promise.resolve("/tmp/shop-web-flow.yaml")) });
    expect(await exportFlow(SECTION, r)).toBe(true);
    expect(r.saveFile).toHaveBeenCalledWith("Export shop-web’s flow", "shop-web-flow.yaml");
    expect(ask).toHaveBeenCalledWith("settings.flow_export", {
      project: "SHOP",
      path: "/tmp/shop-web-flow.yaml",
    });
    expect(r.closed()).toBe(0);
    expect(r.editors).toEqual([]);
    expect(await exportFlow(SECTION, routes())).toBe(false);
  });

  it("Change flow… opens the flow editor on the project and starts nothing", () => {
    const r = routes();
    changeFlow(SECTION, r);
    expect(r.closed()).toBe(1);
    expect(r.editors).toEqual(["SHOP"]);
    expect(r.agentPanel.selection.get().project).toBe("SHOP");
    expect(ask).not.toHaveBeenCalled();
  });
});
