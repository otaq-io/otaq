// @vitest-environment happy-dom
// OTAQ-4 (decisions 052, 053): nothing inside Project settings → Flow scrolls vertically. The stage strip keeps its
// one row and scrolls sideways, but `overflow-x: auto` alone would make it a vertical scroll container too, so it
// clips its vertical axis. happy-dom has no layout: this checks the rules, not the painted bar.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { FlowSettings } from "../../generated/FlowSettings";
import { FlowSectionBody } from "./Flow";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => Promise.resolve(null)),
  pickFolder: vi.fn(),
  pickFlowFile: vi.fn(),
  saveFlowFile: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
  windowAction: vi.fn(),
}));

beforeAll(async () => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  // The test runner leaves CSS imports empty: the settings style sheet, read as the app loads it.
  const fsModule = "node:fs";
  const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
  const here = (import.meta as ImportMeta & { dirname: string }).dirname;
  const style = document.createElement("style");
  style.textContent = fs.readFileSync(`${here}/settings.css`, "utf8");
  document.head.append(style);
});

const stage = (name: string): FlowSettings["rows"][number] => ({
  icon: "bot",
  name,
  caption: "An agent does the work",
  kind: "Agent",
  agent: "Developer",
  model: "Claude Code · sonnet",
  options: "Quick developer",
});

/** Seven stages (wider than the card at 1280 px) and a waiting change: the tallest the card gets. */
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
  rows: ["Product spec", "Tech spec", "Milestone plan", "Spec review", "Dev", "Validate", "Code review"].map(
    stage,
  ),
  problem: null,
  change: { label: "Edit flow", icon: "pencil", disabled: false, hint: null },
  export: { label: "Export", icon: "upload", disabled: false, hint: null },
  back: "Go back to the previous flow",
  menu_label: "More flow actions",
  menu: [],
  export_title: "Export shop-web’s flow",
  export_file: "shop-web-flow.yaml",
  import_title: "Import a flow",
};

/**
 * Whether the browser would let this element scroll up and down. happy-dom reports the declared values, so the CSS
 * rule is applied here: when one axis isn't `visible` (or `clip`), a `visible` other axis computes to `auto`.
 */
function scrollsVertically(style: CSSStyleDeclaration): boolean {
  const visible = (v: string) => v === "" || v === "visible";
  if (["auto", "scroll"].includes(style.overflowY)) return true;
  return visible(style.overflowY) && !visible(style.overflowX) && style.overflowX !== "clip";
}

let root: Root | undefined;
let host: HTMLElement | undefined;

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
});

function draw(): HTMLElement {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() =>
    root?.render(
      createElement(FlowSectionBody, {
        section: SECTION,
        onCancelChange: () => undefined,
        onChange: () => undefined,
        onExport: () => undefined,
        onBack: () => undefined,
        onPreset: () => undefined,
        onCopy: () => undefined,
        onMenu: () => undefined,
      }),
    ),
  );
  const card = host.querySelector<HTMLElement>("section.settings-flow");
  if (!card) throw new Error("no Flow card");
  return card;
}

describe("OTAQ-4: the Flow card never scrolls vertically", () => {
  it("the stage strip scrolls sideways only", () => {
    const strip = draw().querySelector<HTMLElement>(".settings-flow-strip");
    if (!strip) throw new Error("no stage strip");
    expect(strip.querySelectorAll(".settings-flow-step")).toHaveLength(9);
    const style = getComputedStyle(strip);
    expect(style.overflowX).toBe("auto");
    expect(style.overflowY).toBe("hidden");
  });

  it("nothing in the card, the card included, is a vertical scroll container", () => {
    const card = draw();
    const scrolling = [card, ...card.querySelectorAll<HTMLElement>("*")]
      .filter((el) => scrollsVertically(getComputedStyle(el)))
      .map((el) => el.className || el.tagName);
    expect(scrolling).toEqual([]);
  });
});
