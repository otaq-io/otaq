// @vitest-environment happy-dom
// A comment on a diff line is drawn as Markdown, like the thread's (tuq 110, plan comments-as-markdown piece 2), and
// Edit opens it exactly as written.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { DiffAnchor } from "../../generated/DiffAnchor";
import type { DiffHunkView } from "../../generated/DiffHunkView";
import type { FilesComments } from "../../generated/FilesComments";
import { Hunk, useComments } from "./Hunks";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => ({ ready: new Promise(() => undefined), stop: () => undefined })),
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

const ANCHOR: DiffAnchor = { file: "src/export.rs", line: 21, side: "new", key: "src/export.rs:new:21" };

const HUNK: DiffHunkView = {
  header: "@@ -20,2 +20,2 @@",
  lines: [
    {
      kind: "added",
      old: null,
      new: 21,
      sign: "+",
      anchor: ANCHOR,
      spans: [{ token: "plain", text: "let rows = 0;" }],
    },
  ],
  rows: [[null, 0]],
};

const BODY = [
  "**Off by one:** `rows` starts at 0, so the header counts.",
  "- start at 1",
  "- or skip the header",
  "",
  "Returns `Option<usize>`, not <usize>.",
].join("\n");

const COMMENTS: FilesComments = {
  key: "SHOP-34",
  can_comment: true,
  pending: 1,
  threads: [
    {
      anchor: ANCHOR,
      comments: [
        {
          id: "c1",
          author: "You",
          meta: "on line 21 · not sent yet",
          pending: true,
          editable: true,
          body: BODY,
        },
      ],
    },
  ],
  texts: {
    placeholder: "Comment on this line",
    hint: "Goes with Send back, to Dev",
    add: "Add comment",
    save: "Save",
    cancel: "Cancel",
    reply: "Reply",
    edit: "Edit",
    delete: "Delete",
    add_on_line: "Add a comment on line",
  },
};

function Diff() {
  const ctx = useComments("SHOP-34", COMMENTS);
  return createElement(Hunk, { hunk: HUNK, sideBySide: false, ctx });
}

async function draw(): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.append(host);
  await act(async () => {
    root = createRoot(host);
    root.render(createElement(Diff));
    await Promise.resolve();
  });
  return host;
}

describe("a comment on a diff line (tuq 110)", () => {
  it("is drawn as Markdown: bold, code, a list, its line breaks, and a tag shown as text", async () => {
    const host = await draw();
    const body = host.querySelector(".diff-comment .diff-comment-body.md-comment .ProseMirror");
    expect(body).not.toBeNull();
    expect(body?.querySelector("strong")?.textContent).toBe("Off by one:");
    expect([...(body?.querySelectorAll("code") ?? [])].map((c) => c.textContent)).toEqual([
      "rows",
      "Option<usize>",
    ]);
    expect([...(body?.querySelectorAll("li") ?? [])].map((li) => li.textContent)).toEqual([
      "start at 1",
      "or skip the header",
    ]);
    expect(body?.textContent).toContain("not <usize>.");
    expect(body?.querySelector("usize")).toBeNull();
    expect(body?.textContent).not.toContain("**");
  });

  it("opens Edit with the text exactly as written", async () => {
    const host = await draw();
    const edit = [...host.querySelectorAll<HTMLButtonElement>(".diff-link")].find(
      (b) => b.textContent === "Edit",
    );
    await act(async () => {
      edit?.click();
      await Promise.resolve();
    });
    expect(host.querySelector<HTMLTextAreaElement>(".diff-comment textarea")?.value).toBe(BODY);
  });
});
