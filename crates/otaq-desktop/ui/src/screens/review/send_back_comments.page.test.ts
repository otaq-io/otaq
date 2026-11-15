// @vitest-environment happy-dom
// Send back lists the line comments that go with it, each drawn as Markdown like the thread's (tuq 110, plan
// comments-as-markdown piece 2).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, expect, test, vi } from "vitest";

import type { SendBackView } from "../../generated/SendBackView";
import { request } from "../../tauri";
import { SendBackDialog } from "./Dialogs";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

let root: Root | null = null;
afterEach(() => {
  act(() => root?.unmount());
  root = null;
  document.body.innerHTML = "";
});

const VIEW: SendBackView = {
  key: "SHOP-18",
  title: "Send SHOP-18 back to Dev?",
  intro: "Dev gets your comments.",
  pr: null,
  receive_label: "Dev receives",
  comments: [
    { at: "api/checkout/route.ts · L9", body: "**Off by one:** use `rows + 1`.\nThen rerun <tests>." },
    { at: "api/checkout/route.ts · L21", body: "Plain text, as written." },
  ],
  no_comments: "No line comments.",
  comment: {
    label: "Comment for Dev (optional)",
    placeholder: "Add a comment…",
    submit: null,
    footer: null,
    footer_icon: "arrow-right",
    pasted: "Pasted text",
    remove_file: "Remove the file",
    open: "Open",
    remove: "Remove the pasted text",
    attach: "Attach",
    attach_title: "Attach files to the comment",
    optional: true,
    limits: { chars: 2000, report_bytes: 20 * 1024 * 1024, report_label: "20 MB" },
    tell_agent: null,
  },
  waiting_title: null,
  waiting: [],
  footnote: null,
  confirm: "Send back to Dev",
  cancel: "Cancel",
  hint: "cancel · ⌘↵ send",
};

test("each listed comment is drawn as Markdown under where it sits", async () => {
  vi.mocked(request).mockImplementation((method: string) =>
    method === "review.send_back_view" ? Promise.resolve(VIEW) : new Promise(() => undefined),
  );
  const host = document.createElement("div");
  document.body.append(host);
  await act(async () => {
    root = createRoot(host);
    root.render(createElement(SendBackDialog, { params: { ticket: "SHOP-18" }, close: () => undefined }));
    await new Promise((r) => setTimeout(r, 0));
  });
  const listed = [...document.querySelectorAll(".review-comment")];
  expect(listed.map((c) => c.querySelector(".review-comment-at")?.textContent)).toEqual([
    "api/checkout/route.ts · L9",
    "api/checkout/route.ts · L21",
  ]);
  const first = listed[0]?.querySelector(".md-comment .ProseMirror");
  expect(first?.querySelector("strong")?.textContent).toBe("Off by one:");
  expect(first?.querySelector("code")?.textContent).toBe("rows + 1");
  // The single line break is kept, and the tag-like word stays text.
  expect(first?.querySelector("br")).not.toBeNull();
  expect(first?.textContent).toContain("Then rerun <tests>.");
  expect(first?.querySelector("tests")).toBeNull();
  expect(listed[1]?.querySelector(".md-comment .ProseMirror")?.textContent).toBe("Plain text, as written.");
});
