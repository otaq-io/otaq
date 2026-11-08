// @vitest-environment happy-dom
// A start when git has no name and email (an agent never commits under a borrowed name): Start asks for them first,
// "Save and start" saves them (`tools.git_identity`, only on that click) and starts again; Cancel starts nothing and
// writes nothing. The box for new work's Create & start asks the same way before creating anything.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";

import { overlays } from "../../app/overlays";
import type { GitIdentityForm } from "../../generated/GitIdentityForm";
import { request } from "../../tauri";
import { createTicket } from "../new_work/logic";
import { GitIdentityDialog } from "../settings/GitIdentity";
import { startTicket } from "./Board";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => Promise.resolve(null)),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const FORM: GitIdentityForm = {
  title: "git doesn’t know your name and email yet",
  text: "Every commit carries its author’s name and email: the agents’ commits and your merges. Without them git refuses to commit.",
  name_label: "Name",
  name: "",
  name_placeholder: "Ada Lovelace",
  email_label: "Email",
  email: "",
  email_placeholder: "ada@example.com",
  note: "Saved in your git settings for every repository (git config --global).",
  save: "Save",
};

const IDENTITY = { kind: "identity", form: FORM, save: "Save and start", cancel: "Cancel" } as const;

let host: HTMLDivElement | null = null;
let root: Root | null = null;

beforeEach(() => {
  vi.mocked(request).mockClear();
  overlays.store.set([]);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root?.unmount();
  });
  host?.remove();
  overlays.store.set([]);
});

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Draws the identity question the start opened, as the overlay host would. */
function showAsked(): () => void {
  const [open] = overlays.store.get();
  expect(open?.id).toBe("git_identity");
  let closed = 0;
  act(() => {
    root?.render(
      createElement(GitIdentityDialog, {
        params: open?.params as Parameters<typeof GitIdentityDialog>[0]["params"],
        close: () => {
          closed += 1;
        },
      }),
    );
  });
  return () => {
    expect(closed).toBe(1);
  };
}

function type(input: HTMLInputElement | undefined, value: string) {
  act(() => {
    if (input === undefined) return;
    Reflect.set(HTMLInputElement.prototype, "value", value, input);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function button(label: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll("button")].find((b) => b.textContent === label);
}

test("Start asks for git's name and email first; Save and start saves them, then starts", async () => {
  const calls: [string, unknown][] = [];
  let starts = 0;
  const ask = ((method: string, params: unknown) => {
    calls.push([method, params]);
    if (method === "board.start") {
      starts += 1;
      return Promise.resolve(starts === 1 ? IDENTITY : { kind: "moved" });
    }
    return Promise.resolve(null);
  }) as typeof request;
  await startTicket("TALL-1", ask);
  expect(calls).toEqual([["board.start", { key: "TALL-1" }]]);
  const wasClosed = showAsked();
  expect(host?.textContent).toContain("git doesn’t know your name and email yet");
  expect(button("Save and start")).toBeDefined();
  const inputs = [...(host?.querySelectorAll<HTMLInputElement>(".git-identity-fields input") ?? [])];
  expect(inputs).toHaveLength(2);
  type(inputs[0], "Ada Lovelace");
  type(inputs[1], "ada@example.com");
  // Nothing was written before the click.
  expect(calls.map(([m]) => m)).toEqual(["board.start"]);
  await act(async () => {
    button("Save and start")?.click();
    await tick();
  });
  wasClosed();
  expect(vi.mocked(request).mock.calls).toContainEqual([
    "tools.git_identity",
    { name: "Ada Lovelace", email: "ada@example.com" },
  ]);
  expect(starts).toBe(2);
});

test("Cancel starts nothing and writes nothing", async () => {
  const calls: string[] = [];
  const ask = ((method: string) => {
    calls.push(method);
    return Promise.resolve(method === "board.start" ? IDENTITY : null);
  }) as typeof request;
  await startTicket("TALL-1", ask);
  const wasClosed = showAsked();
  act(() => {
    button("Cancel")?.click();
  });
  wasClosed();
  expect(calls).toEqual(["board.start"]);
  expect(vi.mocked(request).mock.calls.filter(([m]) => m === "tools.git_identity")).toEqual([]);
});

test("Create & start from the box asks before creating anything; Cancel keeps what was typed", async () => {
  const calls: string[] = [];
  let creates = 0;
  const ask = ((method: string) => {
    calls.push(method);
    if (method === "editor.create_and_start") {
      creates += 1;
      return Promise.resolve(
        creates < 3
          ? { outcome: "needs_identity", form: FORM, save: "Save and start", cancel: "Cancel" }
          : { outcome: "saved", key: "TALL-2", note: null },
      );
    }
    return Promise.resolve(null);
  }) as typeof request;
  const typed = { title: "Tidy the README", details: "" };
  const kept = createTicket("TALL", typed, true, ask);
  await tick();
  const [open] = overlays.store.get();
  expect(open?.id).toBe("git_identity");
  const params = open?.params as { then: () => void; onCancel?: () => void };
  params.onCancel?.();
  expect(await kept).toEqual({ clear: false, note: null });
  expect(creates).toBe(1);

  overlays.store.set([]);
  const started = createTicket("TALL", typed, true, ask);
  await tick();
  const [again] = overlays.store.get();
  (again?.params as { then: () => void }).then();
  expect(await started).toEqual({ clear: true, note: null });
  expect(creates).toBe(3);
});
