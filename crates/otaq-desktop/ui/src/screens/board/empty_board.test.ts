// @vitest-environment happy-dom
// The empty board's hints are buttons (d0126, finding 5 of the 2026-10-24 dogfood report): "⌘L ask the Project agent"
// runs what ⌘L runs (the panel opens with its box focused), so the keyboard never stays on the board.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it } from "vitest";

import { createRegistries, type Registries, RegistriesContext } from "../../app/registry";
import type { EmptyBoardView } from "../../generated/EmptyBoardView";
import { EmptyBoard } from "./EmptyBoard";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const VIEW: EmptyBoardView = {
  title: "demo-repository is ready",
  text: "Tell the Project agent on the left what you want: it reads the code and writes the tickets. Or write the first one yourself.",
  buttons: [{ label: "New ticket", icon: "plus", primary: true, action: { action: "new_ticket" } }],
  hints: [
    { keys: "⌘N", label: "new ticket", action: { action: "new_ticket" } },
    { keys: "⌘L", label: "ask the Project agent", action: { action: "focus_pane" } },
  ],
};

let root: Root;
let box: HTMLElement;
let app: Registries;
let ran: string[];

beforeEach(() => {
  app = createRegistries();
  ran = [];
  for (const name of ["new_ticket", "focus_pane"] as const)
    app.shortcuts.on(name, () => {
      ran.push(name);
    });
  box = document.body.appendChild(document.createElement("div"));
  root = createRoot(box);
  act(() => {
    root.render(createElement(RegistriesContext, { value: app }, createElement(EmptyBoard, { view: VIEW })));
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
});

it("each hint is a button that runs its key's action", () => {
  const hints = [...box.querySelectorAll<HTMLButtonElement>("button.board-empty-hint")];
  expect(hints.map((b) => b.textContent)).toEqual(["⌘Nnew ticket", "⌘Lask the Project agent"]);
  act(() => {
    hints[1]?.click();
  });
  expect(ran).toEqual(["focus_pane"]);
  act(() => {
    hints[0]?.click();
  });
  expect(ran).toEqual(["focus_pane", "new_ticket"]);
});
