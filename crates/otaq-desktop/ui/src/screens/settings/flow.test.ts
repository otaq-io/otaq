// @vitest-environment happy-dom
// The project's Flow card (FLW-06…FLW-11): it draws Rust's strip and waiting line; every action but Export puts its
// flow in the draft and opens the flow editor on it (FED-09); Export and Import go through the system's dialogs and a
// cancelled dialog does nothing; Edit flow opens the editor and starts nothing; a preset card proposes its file. (The
// flow banner's Apply now and Cancel change: `notices/applyNow.test.ts`.)
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
  intro: "The stages every ticket goes through, and who works in each. Backlog and Done are fixed.",
  waiting: {
    text: "A flow change is waiting: QA after Security. 8 tickets are still working, and no new ticket starts until they’re done.",
    cancel_label: "Cancel change",
  },
  first: "Backlog",
  last: "Done",
  rows: [
    {
      icon: "bot",
      name: "Dev",
      caption: "An agent does the work",
      kind: "Agent",
      agent: "Rust developer",
      model: "Claude Code · sonnet",
      options: "Quick developer",
    },
    {
      icon: "user-check",
      name: "Review",
      caption: "You check it",
      kind: "Check · you",
      agent: "You",
      model: null,
      options: "skip",
    },
  ],
  problem: null,
  change: { label: "Edit flow", icon: "pencil", disabled: false, hint: null },
  export: { label: "Export", icon: "upload", disabled: false, hint: null },
  back: "Go back to the previous flow",
  menu_label: "More flow actions",
  menu: [
    item("preset", "Use a preset…"),
    item("copy", "Copy from another project…"),
    item("import", "Import flow…"),
    item("reset", "Reset to the default flow", { separated: true }),
  ],
  export_title: "Export shop-web’s flow",
  export_file: "shop-web-flow.yaml",
  import_title: "Import a flow",
};

const ask = vi.mocked(request);
let box: HTMLElement;
let root: Root;

function routes(over: Partial<FlowRoutes> = {}): FlowRoutes & { editors: string[] } {
  const editors: string[] = [];
  return {
    ask,
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
  it("draws who changed it, the waiting line, the strip and the actions", () => {
    const cancel = vi.fn();
    const back = vi.fn();
    const exported = vi.fn();
    act(() => {
      root.render(
        createElement(FlowSectionBody, {
          section: SECTION,
          onCancelChange: cancel,
          onChange: vi.fn(),
          onExport: exported,
          onBack: back,
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
      "Rust developer",
      "An agent does the work",
      "You check it",
      "Edit flow",
      "Export",
      "Go back to the previous flow",
    ])
      expect(text).toContain(words);
    const steps = [...box.querySelectorAll(".settings-flow-step .settings-flow-name")].map(
      (s) => s.textContent,
    );
    expect(steps).toEqual(["Backlog", "Dev", "Review", "Done"]);
    const click = (label: string) => {
      const button = [...box.querySelectorAll("button")].find((b) => b.textContent === label);
      act(() => {
        button?.click();
      });
    };
    click("Cancel change");
    expect(cancel).toHaveBeenCalledOnce();
    click("Go back to the previous flow");
    expect(back).toHaveBeenCalledOnce();
    click("Export");
    expect(exported).toHaveBeenCalledOnce();
  });

  it("opens the ⋯ menu with Use a preset, Copy, Import and Reset", () => {
    const onMenu = vi.fn();
    const onPreset = vi.fn();
    act(() => {
      root.render(
        createElement(FlowSectionBody, {
          section: SECTION,
          onCancelChange: vi.fn(),
          onChange: vi.fn(),
          onExport: vi.fn(),
          onBack: vi.fn(),
          onPreset,
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
      "Use a preset…",
      "Copy from another project…",
      "Import flow…",
      "Reset to the default flow",
    ]);
    act(() => {
      document.querySelector<HTMLElement>('[role="menuitem"]')?.click();
    });
    expect(onPreset).toHaveBeenCalledOnce();
    expect(onMenu).not.toHaveBeenCalled();
  });

  it("a preset card proposes its file", () => {
    const dialog: PresetDialogView = {
      project: "SHOP",
      title: "Use a preset for shop-web",
      cancel: "Cancel",
      presets: [
        {
          name: "Spec first",
          delivers: "Done · click",
          line: "Plan on the board, then build",
          columns: [
            { icon: "circle-dashed", label: "Backlog" },
            { icon: "bot", label: "Product spec" },
            { icon: "git-merge", label: "Done" },
          ],
          file: "spec-first.yaml",
        },
      ],
    };
    const onPick = vi.fn();
    act(() => {
      root.render(createElement(PresetDialog, { dialog, onPick, onCancel: vi.fn() }));
    });
    expect(document.body.textContent).toContain("Use a preset for shop-web");
    expect(document.body.textContent).toContain("spec-first.yaml");
    act(() => {
      document.querySelector<HTMLButtonElement>(".settings-preset")?.click();
    });
    expect(onPick).toHaveBeenCalledWith("spec-first.yaml");
  });
});

describe("the Flow actions", () => {
  it("an action's flow lands in the draft and the editor opens on it; a refusal opens nothing", async () => {
    const r = routes();
    await proposeFrom("SHOP", { from: "preset", file: "spec-first.yaml" }, r);
    expect(ask).toHaveBeenCalledWith("settings.flow_propose", {
      project: "SHOP",
      source: { from: "preset", file: "spec-first.yaml" },
    });
    expect(r.editors).toEqual(["SHOP"]);
    ask.mockRejectedValueOnce({ message: "SHOP has no previous flow" });
    await expect(proposeFrom("SHOP", { from: "previous" }, r)).rejects.toBeTruthy();
    expect(ask).toHaveBeenLastCalledWith("settings.flow_propose", {
      project: "SHOP",
      source: { from: "previous" },
    });
    expect(r.editors).toEqual(["SHOP"]);
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
    expect(picked.editors).toEqual(["SHOP"]);
  });

  it("Export saves the flow's file where the dialog said and opens nothing", async () => {
    const r = routes({ saveFile: vi.fn(() => Promise.resolve("/tmp/shop-web-flow.yaml")) });
    expect(await exportFlow(SECTION, r)).toBe(true);
    expect(r.saveFile).toHaveBeenCalledWith("Export shop-web’s flow", "shop-web-flow.yaml");
    expect(ask).toHaveBeenCalledWith("settings.flow_export", {
      project: "SHOP",
      path: "/tmp/shop-web-flow.yaml",
    });
    expect(r.editors).toEqual([]);
    expect(await exportFlow(SECTION, routes())).toBe(false);
  });

  it("Edit flow opens the flow editor on the project and starts nothing", () => {
    const r = routes();
    changeFlow(SECTION, r);
    expect(r.editors).toEqual(["SHOP"]);
    expect(r.agentPanel.selection.get().project).toBe("SHOP");
    expect(ask).not.toHaveBeenCalled();
  });
});
