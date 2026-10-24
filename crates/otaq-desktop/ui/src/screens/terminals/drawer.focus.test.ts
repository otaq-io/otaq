// @vitest-environment happy-dom
// The Terminals drawer's + (d0126, finding 40 of the 2026-10-24 dogfood report): the new terminal is asked for the
// keyboard as soon as the service lists it, which it takes once it opens (`terminal/Terminal.focus.test.ts`), so
// what the user types goes to the shell, not the board.
import { act, createElement, useImperativeHandle } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { createRegistries, RegistriesContext } from "../../app/registry";
import type { DrawerTab } from "../../generated/DrawerTab";
import type { DrawerView } from "../../generated/DrawerView";
import { request } from "../../tauri";
import type { TerminalHandle, TerminalProps } from "../../terminal/Terminal";
import { DrawerBody } from "./Drawer";
import { tabRequests } from "./logic";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn() }));

// The real terminal needs a canvas; this one records which session was asked for the keyboard.
const focused = vi.hoisted(() => [] as string[]);
vi.mock("../../terminal/Terminal", () => ({
  Terminal: ({ session, ref }: TerminalProps) => {
    useImperativeHandle(ref, () => ({ focus: () => focused.push(session) }) as unknown as TerminalHandle, [
      session,
    ]);
    return createElement("div", { className: "fake-terminal" });
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tab = (n: number): DrawerTab => ({
  id: `terminal:t${String(n)}`,
  label: `Terminal ${String(n)}`,
  note: null,
  icon: "square-terminal",
  session: `s${String(n)}`,
  ended: false,
  hint: [],
  ended_line: null,
});

const view = (tabs: DrawerTab[]): DrawerView => ({
  title: "Terminals",
  tabs,
  selected: "terminal:t1",
  new_label: "New terminal",
  close_tab: "Close tab",
  hide: "Hide · terminals keep running",
  maximize: "Maximize",
  restore: "Restore",
  empty: [],
  height: 380,
});

let root: Root;
let box: HTMLElement;

beforeEach(() => {
  focused.length = 0;
  box = document.body.appendChild(document.createElement("div"));
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
  tabRequests.set(null);
});

function show(v: DrawerView) {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: createRegistries() },
        createElement(DrawerBody, {
          view: v,
          project: "SHOP",
          maximized: false,
          onMaximize: () => undefined,
          onHide: () => undefined,
        }),
      ),
    );
  });
}

it("+ asks the new terminal for the keyboard once the service lists it, once", async () => {
  vi.mocked(request).mockResolvedValue("terminal:t2");
  show(view([tab(1)]));
  await act(async () => {
    box.querySelector<HTMLButtonElement>('button[aria-label="New terminal"]')?.click();
    await Promise.resolve();
  });
  expect(request).toHaveBeenCalledWith("drawer.new", { project: "SHOP" });
  // Not listed yet: nothing to focus.
  expect(focused).toEqual([]);
  show(view([tab(1), tab(2)]));
  expect(focused).toEqual(["s2"]);
  show(view([tab(1), tab(2)]));
  expect(focused).toEqual(["s2"]);
});
