// @vitest-environment happy-dom
// Approve & open PR's commit message fields: Open PR shows each commit's message as Push to PR does, marks the one the
// project's commit-msg hook refused last time, and sends each edit to Rust (`pr.edit`), which keeps it on the ticket.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PrAnswer } from "../../generated/PrAnswer";
import type { PrDialogView } from "../../generated/PrDialogView";
import { request } from "../../tauri";
import { OpenPrDialog } from "./Dialogs";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** `pr_page.rs`'s Open PR confirmation after the commit-msg hook refused "update stuff" (kept on the ticket). */
const VIEW: PrDialogView = {
  key: "SHOP-1",
  push: false,
  heading: "Open a pull request for SHOP-1",
  stage: { kind: "proposed" },
  proposed_by: "",
  provider: null,
  login: null,
  branch_label: "Branch",
  branch: "feature/SHOP-1-login",
  branch_locked: false,
  branch_hint: { text: "Valid name · not taken on origin", error: false },
  check: 0,
  check_after_ms: 300,
  commits_label: "Commit messages (2 commits)",
  commits: [
    { sha: "a1b2c3d4", message: "update stuff", refused: true },
    { sha: "e5f6a7b8", message: "fix: cover deep links", refused: false },
  ],
  title_label: "Title",
  title: "SHOP-1 Login redirect",
  description_label: "Description",
  description: "Returns to the page you came from.",
  target_line: "main",
  confirm: "Push & open PR",
  can_confirm: true,
  cancel: "Cancel",
  hint: "cancel",
  prepared: null,
  writing: null,
  asks: null,
  set_up: null,
  merge_instead: null,
  filled: 1,
  use_proposed: "Use the proposed message",
};

let root: Root;
let box: HTMLElement;

beforeEach(() => {
  box = document.createElement("div");
  document.body.append(box);
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
  vi.mocked(request).mockReset();
});

function answer(view: PrDialogView): PrAnswer {
  return { outcome: "dialog", view, intro: null };
}

const field = (sha: string) => document.querySelector<HTMLInputElement>(`input[aria-label="${sha}"]`);

describe("Open PR's commit messages", () => {
  it("shows each commit's field with the refused one marked, and sends an edit to Rust", async () => {
    vi.mocked(request).mockImplementation((method: string) =>
      Promise.resolve(
        method === "pr.edit"
          ? answer({
              ...VIEW,
              commits: [
                { sha: "a1b2c3d4", message: "fix: redirect after login", refused: false },
                { sha: "e5f6a7b8", message: "fix: cover deep links", refused: false },
              ],
            })
          : answer(VIEW),
      ),
    );
    await act(async () => {
      root.render(createElement(OpenPrDialog, { params: { ticket: "SHOP-1" }, close: () => undefined }));
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("pr.open", { key: "SHOP-1", approve_anyway: false, intro: false });
    expect(document.body.textContent).toContain("Commit messages (2 commits)");
    const refused = field("a1b2c3d4");
    expect(refused?.value).toBe("update stuff");
    expect(refused?.getAttribute("aria-invalid")).toBe("true");
    expect(field("e5f6a7b8")?.value).toBe("fix: cover deep links");
    expect(field("e5f6a7b8")?.hasAttribute("aria-invalid")).toBe(false);
    // The title and description stay beside them.
    expect(document.body.textContent).toContain("Title");

    await act(async () => {
      // The prototype's setter, so React's value tracker sees a change.
      Reflect.set(HTMLInputElement.prototype, "value", "fix: redirect after login", refused);
      refused?.dispatchEvent(new Event("input", { bubbles: true }));
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith("pr.edit", {
      key: "SHOP-1",
      field: "commit",
      index: 0,
      value: "fix: redirect after login",
    });
    expect(field("a1b2c3d4")?.value).toBe("fix: redirect after login");
    expect(field("a1b2c3d4")?.hasAttribute("aria-invalid")).toBe(false);
  });
});
