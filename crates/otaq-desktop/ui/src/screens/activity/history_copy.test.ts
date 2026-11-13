// @vitest-environment happy-dom
// History's quiet session row: it shows the start of the agent session's tool id and its button copies all of it, so
// the user can resume that session by hand.
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { beforeAll, describe, expect, it, vi } from "vitest";

import type { HistoryView } from "../../generated/HistoryView";
import { HistoryBody } from "./History";

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const ID = "c41e9a07-5b2d-4e8f-9a01-23456789abcd";

const VIEW: HistoryView = {
  filters: [{ filter: "all", label: "All", kind: null }],
  order: "newest first",
  copy_hint: "Copy session ID",
  copied: "Copied",
  empty: "Nothing has happened to this ticket yet.",
  total: null,
  key: "SHOP-18",
  rows: [
    {
      title: "Session · Validate · Verifier",
      detail: "Claude Code · sonnet",
      when: "10:22",
      who: "10:22",
      cost: null,
      tool_session_id: ID,
      quiet: true,
      short_id: "c41e9a07-…",
      range: "10:22–10:40",
      mark: { kind: "agent" },
      opens: null,
      hook_changes: null,
      kind: "session",
    },
  ],
};

describe("History's session row", () => {
  it("shows the session's id and copies all of it", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const host = document.createElement("div");
    document.body.append(host);
    await act(async () => {
      createRoot(host).render(createElement(HistoryBody, { view: VIEW }));
      await Promise.resolve();
    });
    expect(host.querySelector(".history-session-id")?.textContent).toBe("c41e9a07-…");
    expect(host.querySelector(".history-session-range")?.textContent).toBe("10:22–10:40");
    const copy = host.querySelector<HTMLButtonElement>(
      '.history-session button[aria-label="Copy session ID"]',
    );
    expect(copy).not.toBeNull();
    await act(async () => {
      copy?.click();
      await Promise.resolve();
    });
    expect(writeText).toHaveBeenCalledWith(ID);
  });
});
