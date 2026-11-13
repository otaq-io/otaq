// @vitest-environment happy-dom
// The after-restart dialog's Resume: once every row it asked for resumed, the dialog closes (the re-test saw it stay
// open on "Resumed" and a greyed "Resume 0" until Not now); when one couldn't resume, it stays open on its Retry. The
// service is a stand-in.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { AfterRestartRow } from "../../generated/AfterRestartRow";
import type { AfterRestartSummary } from "../../generated/AfterRestartSummary";
import { request } from "../../tauri";
import { AfterRestart } from "./AfterRestart";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

function row(id: string, extra: Partial<AfterRestartRow> = {}): AfterRestartRow {
  return {
    id,
    kind: "ticket",
    tag: id,
    title: "Wishlist share link",
    state: "stopped",
    detail: "was working in Dev",
    problem: null,
    sign_in: null,
    tool: "claude",
    checking: null,
    checked: true,
    ...extra,
  };
}

const SUMMARY: AfterRestartSummary = {
  title: "2 agents were stopped by a restart",
  text: "Each can continue its same conversation, in its own copy. Resuming uses tokens.",
  rows: [row("SHOP-30"), row("SHOP-29", { detail: "waits for your answer", checked: false })],
  notes: ["SHOP-29 still waits for your answer; answering resumes it."],
  retry: "Retry",
  scratch: "Start from scratch",
  resuming: "Resuming…",
  foot: "They wait in Needs you if you close this.",
  later: "Not now",
  resume_selected: "Resume 1",
  selected: ["SHOP-30"],
  selected_agents: [],
  terminal: {
    hide: "Hide",
    close: "Close",
    retry: "Retry",
    checking: "Ended · checking again…",
    not_done: "Not done yet — see the output above",
    done: "Done",
  },
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
  document.body.innerHTML = "";
  vi.mocked(request).mockReset();
});

/** The dialog over the stand-in service, whose Resume answers `resumed`; its `close` is counted. */
async function open(resumed: number) {
  vi.mocked(request).mockImplementation((method: string) =>
    Promise.resolve(method === "recovery.summary" ? SUMMARY : method === "recovery.resume" ? resumed : null),
  );
  const close = vi.fn();
  await act(async () => {
    root.render(createElement(AfterRestart, { params: undefined as never, close }));
    await Promise.resolve();
  });
  return close;
}

async function resume() {
  const button = [...document.querySelectorAll("button")].find((b) => b.textContent.includes("Resume 1"));
  expect(button).toBeDefined();
  await act(async () => {
    button?.click();
    await Promise.resolve();
  });
}

describe("the after-restart dialog's Resume", () => {
  it("closes once every ticked row resumed", async () => {
    const close = await open(1);
    expect(close).not.toHaveBeenCalled();
    await resume();
    expect(vi.mocked(request)).toHaveBeenCalledWith("recovery.resume", { keys: ["SHOP-30"], agents: [] });
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("stays open when a row couldn't resume, for its Retry", async () => {
    const close = await open(0);
    await resume();
    expect(close).not.toHaveBeenCalled();
  });
});
