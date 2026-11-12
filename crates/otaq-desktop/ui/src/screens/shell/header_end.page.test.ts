// @vitest-environment happy-dom
// NEW-b1: Running counts only agents at work, so it leaves the title bar while the only agent waits at its prompt,
// and Pause stays. Pause must not slide into the place Running had: the usage chip, Running and Pause are held at
// the title bar's right end, Pause last, so Running leaving moves nothing after it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import type { HeaderView } from "../../generated/HeaderView";
import { HeaderBar } from "./Header";

beforeAll(async () => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  // The test runner leaves CSS imports empty: the header's own style sheet, read as the app loads it.
  const fsModule = "node:fs";
  const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
  const here = (import.meta as ImportMeta & { dirname: string }).dirname;
  const style = document.createElement("style");
  style.textContent = fs.readFileSync(`${here}/shell.css`, "utf8");
  document.head.append(style);
});

const WORKING: HeaderView = {
  project: null,
  search: { text: "Search", keys: "/" },
  activity: {
    shown: true,
    label: "Running",
    agents: { count: "1", label: "running", control: false, active: true, paused: false },
    processes: null,
    hover: { title: "1 agent running", lines: [] },
    warning: null,
    keys: "⇧⌘U",
  },
};

/** The same title bar once the agent waits at its prompt: not counted, so Running goes; Pause stays. */
const WAITING: HeaderView = {
  ...WORKING,
  activity: {
    ...WORKING.activity,
    shown: false,
    agents: { count: "0", label: "running", control: false, active: false, paused: false },
  },
};

let root: Root | undefined;
let host: HTMLElement | undefined;

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
});

function draw(view: HeaderView): HTMLElement {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  const pause = createElement("button", { type: "button", className: "header-pause" }, "Pause");
  act(() => root?.render(createElement(HeaderBar, { view, onSearch: () => undefined, pause })));
  return host;
}

describe("NEW-b1: Running leaving the title bar doesn't move Pause", () => {
  for (const [name, view] of [
    ["while an agent works", WORKING],
    ["while the only agent waits at its prompt", WAITING],
  ] as const) {
    it(`Pause is last in the group held at the right end ${name}`, () => {
      const bar = draw(view);
      const pause = bar.querySelector<HTMLElement>(".header-pause");
      const end = pause?.parentElement;
      if (!pause || !end) throw new Error("no Pause");
      expect(end.classList.contains("header-end")).toBe(true);
      expect(end.lastElementChild).toBe(pause);
      // The group's free room is on its left, so whatever leaves it closes in from the left and Pause stays put.
      expect(getComputedStyle(end).marginLeft).toBe("auto");
      expect(bar.querySelector(".header-activity") !== null).toBe(view.activity.shown);
    });
  }
});
