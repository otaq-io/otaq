// @vitest-environment happy-dom
// Apply now (T-0439, APN-01, APN-02): the flow change banner's Apply now opens the window on the project on screen and
// Cancel change asks Rust; in the window a changed pick goes back to Rust for the view and Apply now sends the picks
// with the plan's stamp; Cancel and Esc close it with no request; a stale answer redraws it with the line Rust wrote;
// Keep their place opens on click.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ApplyNowMove } from "../../generated/ApplyNowMove";
import type { ApplyNowView } from "../../generated/ApplyNowView";
import type { MenuItem } from "../../generated/MenuItem";
import { ApplyNowWindow } from "./ApplyNow";
import { pressBanner } from "./notices";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
  windowAction: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const item = (id: string, label: string, checked = false): MenuItem => ({
  id,
  label,
  icon: null,
  shortcut: null,
  danger: false,
  disabled: false,
  hint: null,
  separated: false,
  checked,
  header: false,
});

const PLACES: [string, string][] = [
  ["backlog", "Backlog"],
  ["ready_for_dev", "Ready for Dev"],
  ["stage:dev", "Dev"],
  ["stage:validate", "Validate"],
  ["stage:review", "Review"],
];

const NAMES = Object.fromEntries(PLACES);

/** SHOP-19 in Security on `picked`, as `apply_now.rs` draws it (`s11-17` third case). */
function move(picked: string): ApplyNowMove {
  return {
    ticket: "SHOP-19",
    title: "Price table",
    line: "in Security (removed) · agent will stop",
    picked,
    picked_name: NAMES[picked] ?? "",
    picked_icon: "user-check",
    choices: PLACES.map(([id, label]) => item(id, label, id === picked)),
  };
}

function view(picked = "stage:review", over: Partial<ApplyNowView> = {}): ApplyNowView {
  return {
    project: "SHOP",
    title: "Apply the flow change now?",
    summary: "Removes Security from shop-web’s flow.",
    changed: null,
    moves_label: "Moves",
    moves: [move(picked)],
    keep_label: "Keep their place · 4",
    keep: ["SHOP-15", "SHOP-33", "SHOP-18", "SHOP-23"].map((ticket) => ({
      ticket,
      title: "Work",
      line: "in Dev · keeps running",
    })),
    keep_collapsed: "SHOP-15 · in Dev · keeps running, and 3 more",
    hint: picked === "stage:dev" ? "Back to Dev: Dev resumes SHOP-19’s earlier session." : null,
    cancel: "Cancel",
    apply: "Apply now",
    stamp: "s1",
    basis: { summary: "Removes Security from shop-web’s flow", moves: [], keep: [] },
    ...over,
  };
}

type Params = { picks?: Record<string, string>; changed?: { stamp: string; line: string } };

let box: HTMLDivElement;
let root: Root;
let ask: ReturnType<typeof vi.fn<(method: string, params?: unknown) => Promise<unknown>>>;
let close: ReturnType<typeof vi.fn<() => void>>;
/** What `apply_now.apply` answers next. */
let answer: unknown;

const settle = async () => {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
};

const button = (label: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === label);

const viewCalls = () =>
  ask.mock.calls.filter(([method]) => method === "apply_now.view").map(([, p]) => p as Params);

beforeEach(async () => {
  answer = { outcome: "closed" };
  ask = vi.fn((method: string, params?: unknown) => {
    if (method === "apply_now.apply") return Promise.resolve(answer);
    const p = params as Params;
    const changed = p.changed?.stamp === "s2" ? p.changed.line : null;
    return Promise.resolve({
      view: view(p.picks?.["SHOP-19"] ?? "stage:review", changed === null ? {} : { changed, stamp: "s2" }),
    });
  });
  close = vi.fn<() => void>();
  box = document.body.appendChild(document.createElement("div"));
  root = createRoot(box);
  act(() => {
    root.render(createElement(ApplyNowWindow, { project: "SHOP", close, ask }));
  });
  await settle();
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
});

describe("the flow change banner", () => {
  it("Apply now opens the window on the project on screen; Cancel change asks Rust", async () => {
    const stack = { open: vi.fn() };
    const banner = vi.fn(() => Promise.resolve(null));
    await pressBanner("flow_waiting", "action", { ask: banner, overlays: stack, project: () => "SHOP" });
    expect(stack.open).toHaveBeenCalledWith("apply_now", { project: "SHOP" });
    await pressBanner("flow_waiting", "secondary", { ask: banner, overlays: stack, project: () => "SHOP" });
    expect(banner).toHaveBeenCalledWith("notices.flow_cancel", { project: null });
    expect(stack.open).toHaveBeenCalledTimes(1);
    // No project on screen: nothing to open.
    await pressBanner("flow_waiting", "action", { ask: banner, overlays: stack, project: () => null });
    expect(stack.open).toHaveBeenCalledTimes(1);
  });
});

describe("the Apply now window", () => {
  it("draws Rust's view: title, summary, the move on its default, Keep their place collapsed", () => {
    const text = document.body.textContent;
    expect(text).toContain("Apply the flow change now?");
    expect(text).toContain("Removes Security from shop-web’s flow.");
    expect(text).toContain("in Security (removed) · agent will stop");
    expect(document.querySelector(".select")?.textContent).toContain("Review");
    expect(text).toContain("SHOP-15 · in Dev · keeps running, and 3 more");
    expect(text).not.toContain("SHOP-33");
    expect(viewCalls()[0]).toEqual({ project: "SHOP", picks: {} });
  });

  it("a changed pick redraws with its hint, and Apply now sends the picks with the stamp", async () => {
    act(() => {
      (document.querySelector(".select") as HTMLButtonElement).click();
    });
    const dev = [...document.querySelectorAll('[role="menuitemradio"]')].find(
      (el) => el.textContent.trim() === "Dev",
    ) as HTMLButtonElement;
    act(() => {
      dev.click();
    });
    await settle();
    expect(viewCalls().at(-1)).toEqual({ project: "SHOP", picks: { "SHOP-19": "stage:dev" } });
    expect(document.body.textContent).toContain("Back to Dev: Dev resumes SHOP-19’s earlier session.");
    act(() => {
      button("Apply now")?.click();
    });
    await settle();
    expect(ask).toHaveBeenCalledWith("apply_now.apply", {
      project: "SHOP",
      stamp: "s1",
      picks: { "SHOP-19": "stage:dev" },
      basis: view().basis,
    });
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("Cancel and Esc close it and change nothing", async () => {
    act(() => {
      button("Cancel")?.click();
    });
    expect(close).toHaveBeenCalledTimes(1);
    const dialog = document.querySelector('[role="dialog"]') as HTMLElement;
    act(() => {
      dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    await settle();
    expect(close).toHaveBeenCalledTimes(2);
    expect(ask.mock.calls.map(([method]) => method)).not.toContain("apply_now.apply");
  });

  it("a stale answer redraws it with the line naming what changed", async () => {
    const line = "The board changed: SHOP-41 entered Product spec.";
    answer = { outcome: "refreshed", view: view("stage:review", { stamp: "s2", changed: line }) };
    act(() => {
      button("Apply now")?.click();
    });
    await settle();
    expect(close).not.toHaveBeenCalled();
    expect(viewCalls().at(-1)).toEqual({ project: "SHOP", picks: {}, changed: { stamp: "s2", line } });
    expect(document.querySelector('[role="status"]')?.textContent).toBe(line);
  });

  it("Keep their place opens on click", () => {
    const head = document.querySelector(".apply-now-keep-head") as HTMLButtonElement;
    expect(head.getAttribute("aria-expanded")).toBe("false");
    act(() => {
      head.click();
    });
    expect(head.getAttribute("aria-expanded")).toBe("true");
    expect(document.body.textContent).toContain("SHOP-33");
    expect(document.body.textContent).not.toContain("and 3 more");
  });
});
