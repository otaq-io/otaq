// @vitest-environment happy-dom
// The Open PR dialog while pushing: Cancel and Esc are enabled and stop waiting (`pr.cancel`, then the dialog closes),
// so a delivery turn that never starts can't hold the window; a stray press outside doesn't cancel.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import type { PrAnswer } from "../../generated/PrAnswer";
import type { PrDialogView } from "../../generated/PrDialogView";
import { request } from "../../tauri";
import { OpenPrDialog } from "./Dialogs";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const PUSHING: PrDialogView = {
  key: "SHOP-18",
  push: false,
  heading: "Open PR for SHOP-18 Checkout API",
  stage: {
    kind: "pushing",
    title: "Pushing SHOP-18…",
    steps: [{ text: "Pushing to origin…", state: "running" }],
  },
  proposed_by: "",
  provider: "GitHub · acme/shop-web",
  login: null,
  branch_label: "Branch name",
  branch: "otaq/SHOP-18-checkout-api",
  branch_locked: true,
  branch_hint: { text: "", error: false },
  check: 1,
  check_after_ms: 300,
  commits_label: "Commit messages (1 commit)",
  commits: [{ sha: "a1b2c3d4", message: "feat(checkout): add POST /checkout", refused: false }],
  title_label: "PR title",
  title: "Checkout API (SHOP-18)",
  description_label: "Description",
  description: "",
  target_line: "Into main",
  confirm: "Push & open PR",
  can_confirm: false,
  cancel: "Cancel",
  hint: "cancel",
  prepared: null,
  writing: null,
  asks: null,
  waits: null,
  set_up: null,
  merge_instead: null,
  filled: 0,
  use_proposed: null,
};

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  vi.mocked(request).mockReset();
});

/** The dialog open while pushing; returns its `close`. */
async function openPushing() {
  const answer: PrAnswer = { outcome: "dialog", view: PUSHING, intro: null };
  vi.mocked(request).mockImplementation((method: string) =>
    Promise.resolve(method === "pr.cancel" ? { outcome: "closed" } : answer),
  );
  const close = vi.fn();
  await act(async () => {
    root.render(createElement(OpenPrDialog, { params: { ticket: "SHOP-18" }, close } as never));
    await Promise.resolve();
  });
  return close;
}

test("Cancel while pushing is enabled, asks pr.cancel and closes the dialog", async () => {
  const close = await openPushing();
  expect(document.body.textContent).toContain("Pushing to origin…");
  const cancel = [...document.querySelectorAll("button")].find((b) => b.textContent === "Cancel");
  expect(cancel).toBeDefined();
  expect(cancel?.disabled).toBe(false);
  await act(async () => {
    cancel?.click();
    await Promise.resolve();
  });
  expect(vi.mocked(request)).toHaveBeenCalledWith("pr.cancel", { key: "SHOP-18" });
  expect(close).toHaveBeenCalled();
});

test("Esc while pushing cancels too; a press outside the dialog doesn't", async () => {
  const close = await openPushing();
  const backdrop = document.querySelector(".dialog-backdrop");
  expect(backdrop).not.toBeNull();
  await act(async () => {
    backdrop?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    await Promise.resolve();
  });
  expect(vi.mocked(request)).not.toHaveBeenCalledWith("pr.cancel", expect.anything());
  expect(close).not.toHaveBeenCalled();
  await act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );
    await Promise.resolve();
  });
  expect(vi.mocked(request)).toHaveBeenCalledWith("pr.cancel", { key: "SHOP-18" });
  expect(close).toHaveBeenCalled();
});
