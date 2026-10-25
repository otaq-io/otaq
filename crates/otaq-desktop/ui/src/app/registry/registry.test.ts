import { afterEach, describe, expect, it } from "vitest";

import type { SheetContent } from "../../generated/SheetContent";
import * as screens from "../../screens";
import { sample } from "../../screens/_sample";
import { overlays } from "../overlays";
import { createRegistries, registered, registerScreens } from ".";
import { OVERLAY_IDS, OVERLAY_MODAL, type OverlayId } from "./overlays";
import { SHEET_TABS } from "./sheet";
import { createRegistry } from "./registry";
import { createShortcuts } from "./shortcuts";

afterEach(() => {
  overlays.store.set([]);
});

describe("a screen plugs in", () => {
  it("from its own folder plus one line in the screens index, and nothing else changes", () => {
    const before = registered(registerScreens(screens));
    const app = registerScreens({ ...screens, sample });
    const after = registered(app);

    // Exactly the sample's two registrations were added.
    const added = Object.fromEntries(
      Object.entries(after).map(([registry, ids]) => [
        registry,
        ids.filter((id) => !before[registry]?.includes(id)),
      ]),
    );
    expect(added).toEqual({
      slots: [],
      overlays: [],
      sheetTabs: [],
      sheetSections: [],
      globalSettings: [],
      projectSettings: [],
      shortcuts: ["switch_tab"],
    });

    expect(app.shortcuts.run({ action: "switch_tab", number: 6 })).toBe(true);
    expect(overlays.store.get()).toEqual([{ id: "processes", params: undefined }]);
    // Not the sample's key: no handler takes it.
    expect(app.shortcuts.run({ action: "switch_tab", number: 2 })).toBe(false);
  });

  it("each id has one owner", () => {
    const app = registerScreens({});
    const entry = { Component: () => null };
    app.overlays.register("cleanup", entry);
    expect(() => {
      app.overlays.register("cleanup", entry);
    }).toThrow('overlays: "cleanup" is registered twice');
  });
});

describe("a registry", () => {
  it("lists what is registered in its fixed order", () => {
    const r = createRegistry<Record<"a" | "b" | "c", number>>("letters", ["a", "b", "c"]);
    r.register("c", 3);
    r.register("a", 1);
    expect(r.ids()).toEqual(["a", "c"]);
    expect(r.get("a")).toBe(1);
    expect(r.get("b")).toBeUndefined();
  });

  it("starts empty", () => {
    expect(Object.values(registered(createRegistries())).flat()).toEqual([]);
  });
});

describe("the fixed ids", () => {
  it("sheet tabs: Overview · Session · Files · Activity and an epic's Lead (SHT-02)", () => {
    expect(SHEET_TABS).toEqual(["overview", "session", "files", "activity", "lead", "tickets"]);
  });

  it("overlays: every id has its modality; popovers and drawers don't hold shortcuts back", () => {
    expect(OVERLAY_IDS).toHaveLength(34);
    const nonModal = OVERLAY_IDS.filter((id) => !OVERLAY_MODAL[id]);
    expect(nonModal).toEqual(["quick_switcher", "command_palette", "processes", "terminals", "activity"]);
  });

  // No `default`: an id added to a union without a case here fails ESLint's switch-exhaustiveness-check.
  it("are exhaustive unions", () => {
    const tabIcon = (tab: SheetContent): string => {
      switch (tab) {
        case "agent":
        case "check":
        case "shell":
          return "terminal";
        case "files":
        case "run":
        case "overview":
        case "activity":
          return tab;
        case "lead":
          return "bot";
        case "tickets":
          return "list";
      }
    };
    const kind = (id: OverlayId): string => {
      switch (id) {
        case "quick_switcher":
        case "command_palette":
        case "activity":
          return "popover";
        case "terminals":
        case "processes":
          return "drawer";
        case "new_ticket":
        case "edit_ticket":
        case "settings":
        case "run_command":
        case "cleanup":
        case "after_restart":
        case "add_project":
        case "delete_ticket":
        case "close_ticket":
        case "submit_for_review":
        case "start_from_scratch":
        case "send_back":
        case "drag_back":
        case "blocked":
        case "send_back_pr":
        case "change_decision":
        case "local_merge":
        case "open_pr":
        case "confirm":
        case "restart_service":
        case "resume_all":
        case "usage_data":
        case "shortcuts":
        case "apply_now":
        case "report":
          return "dialog";
        case "gallery":
        case "onboarding":
        case "full_view":
        case "flow_editor":
          return "page";
      }
    };
    const contents: SheetContent[] = [
      "overview",
      "agent",
      "check",
      "run",
      "shell",
      "files",
      "activity",
      "lead",
    ];
    expect(contents.map(tabIcon)).toHaveLength(8);
    expect(OVERLAY_IDS.filter((id) => kind(id) === "dialog")).toHaveLength(25);
  });
});

describe("shortcut handlers", () => {
  it("the newest handler that takes the key wins; removing it hands the key back", () => {
    const shortcuts = createShortcuts();
    const calls: string[] = [];
    shortcuts.on("close_sheet", () => calls.push("sheet"));
    const stop = shortcuts.on("close_sheet", () => {
      calls.push("overlay");
      return false;
    });
    expect(shortcuts.run({ action: "close_sheet" })).toBe(true);
    expect(calls).toEqual(["overlay", "sheet"]);

    const project = shortcuts.on("project", ({ number }) => calls.push(`project ${String(number)}`));
    expect(shortcuts.run({ action: "project", number: 3 })).toBe(true);
    expect(calls.at(-1)).toBe("project 3");

    stop();
    project();
    expect(shortcuts.names()).toEqual(["close_sheet"]);
    expect(shortcuts.run({ action: "project", number: 3 })).toBe(false);
  });

  it("tells its listeners each action a handler took (the palette's recent items)", () => {
    const shortcuts = createShortcuts();
    const ran: string[] = [];
    const stop = shortcuts.onRan(({ action }) => ran.push(action));
    shortcuts.on("park", () => false);
    shortcuts.on("approve", () => undefined);
    expect(shortcuts.run({ action: "park" })).toBe(false);
    expect(shortcuts.run({ action: "approve" })).toBe(true);
    stop();
    shortcuts.run({ action: "approve" });
    expect(ran).toEqual(["approve"]);
  });
});
