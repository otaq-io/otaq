// @vitest-environment happy-dom
// 0.3 F-A-1 / F-B-1 (#21 refined on 2026-11-16): while any ticket sheet is open the box for new work isn't drawn (it
// used to shrink into a pill under the rail beside the floating sheet), ⌘N opens the full form instead, and the box
// comes back once the sheet closes.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { NewWorkView } from "../generated/NewWorkView";
import { newWork } from "../screens/new_work";
import { boxOnScreen, openNewWork } from "../screens/new_work/logic";
import { request } from "../tauri";
import { Frame } from "./Frame";
import { INITIAL_LAYOUT, layoutView, shellLayout } from "./layout";
import { openOverlay, overlays } from "./overlays";
import { RegistriesContext, registerScreens } from "./registry";
import { NOTHING_SELECTED, selection } from "./selection";

vi.mock("../tauri", async (original) => ({
  ...(await original<typeof import("../tauri")>()),
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
  windowAction: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ask = vi.mocked(request);

/** `new_work.rs`'s words on a Mac. */
const VIEW: NewWorkView = {
  shown: true,
  placeholder: "New ticket, or ask Noyan…",
  keys: "⌘N",
  details: "Add details (optional)",
  new_line: { label: "new line", keys: "⌥↵" },
  more_options: "More options…",
  ask: { label: "Ask Noyan to shape it", keys: "⇧↵" },
  create: { label: "Create", keys: "↵" },
  start: { label: "Create & start", keys: "⌘↵" },
};

let root: Root;
let page: HTMLElement;

/** The board's New ticket shortcut (`screens/board/index.tsx`): the box if it is on screen, else the full form. */
const pressNew = () => {
  act(() => {
    openNewWork(() => {
      openOverlay("new_ticket");
    });
  });
};

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

const box = () => page.querySelector(".frame-new-work .new-work-closed");

beforeEach(() => {
  ask.mockImplementation((method: string) =>
    method === "new_work.view" ? Promise.resolve(VIEW) : new Promise<never>(() => undefined),
  );
  page = document.body.appendChild(document.createElement("div"));
  root = createRoot(page);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
  ask.mockReset();
  selection.set(NOTHING_SELECTED);
  overlays.store.set([]);
  shellLayout.set(INITIAL_LAYOUT);
  layoutView.set(undefined);
  boxOnScreen.set(false);
});

describe("the box for new work while a ticket sheet is open (0.3 F-A-1)", () => {
  it("is gone with any sheet open, ⌘N then opens the full form, and it comes back once the sheet closes", async () => {
    selection.set({ ...NOTHING_SELECTED, project: "SHOP" });
    shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: false });
    const app = registerScreens({ newWork });
    act(() => {
      root.render(createElement(RegistriesContext, { value: app }, createElement(Frame)));
    });
    await settle();
    // No sheet: the box at the board's bottom centre, the whole main area wide, and ⌘N opens it.
    expect(box()?.textContent).toContain("New ticket, or ask Noyan…");
    expect(page.querySelector<HTMLElement>(".frame-new-work")?.style.right).toBe("");
    expect(boxOnScreen.get()).toBe(true);
    pressNew();
    expect(page.querySelector(".new-work-open")).not.toBeNull();
    expect(overlays.store.get()).toEqual([]);

    // A ticket's sheet opens (floating, as at 1280-1920 px): no box at all, no pill under the rail.
    act(() => {
      selection.set({ ...selection.get(), ticket: "SHOP-4" });
    });
    await settle();
    expect(page.querySelector('[data-slot="sheet"]')).not.toBeNull();
    expect(page.querySelector('[data-slot="new_work"]')).toBeNull();
    expect(page.querySelector(".new-work-closed, .new-work-open")).toBeNull();
    expect(boxOnScreen.get()).toBe(false);
    // ⌘N: the full form.
    pressNew();
    expect(overlays.store.get()).toEqual([{ id: "new_ticket", params: undefined }]);
    act(() => {
      overlays.store.set([]);
    });

    // Full page: still none.
    act(() => {
      shellLayout.set({ ...shellLayout.get(), sheetExpanded: true });
    });
    expect(page.querySelector('[data-slot="new_work"]')).toBeNull();
    act(() => {
      shellLayout.set({ ...shellLayout.get(), sheetExpanded: false });
    });

    // The sheet closes: the box is back, and ⌘N opens it again.
    act(() => {
      selection.set({ ...selection.get(), ticket: null });
    });
    await settle();
    expect(box()?.textContent).toContain("New ticket, or ask Noyan…");
    expect(boxOnScreen.get()).toBe(true);
    pressNew();
    expect(page.querySelector(".new-work-open")).not.toBeNull();
    expect(overlays.store.get()).toEqual([]);
  });
});
