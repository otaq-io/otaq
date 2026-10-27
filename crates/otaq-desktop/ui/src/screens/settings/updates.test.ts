import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { createStore } from "../../app/store";
import type { UpdatesView } from "../../generated/UpdatesView";
import { createPageEvents } from "../../service";
import { UpdateNoticeBox, UpdateRows } from "./About";
import { updatesClick, watchUpdates } from "./updates";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

function view(dot: string | null): UpdatesView {
  return {
    auto: { label: "Check for updates automatically", description: "d", on: true },
    row: {
      label: "Updates",
      result: null,
      plain: null,
      check: { label: "Check for updates", enabled: true },
    },
    notice: null,
    dot,
  };
}

/** An event stream the test feeds by hand; `open()` resolves the subscription. */
function fakeStream() {
  let send: (event: unknown) => void = () => undefined;
  let open: () => void = () => undefined;
  const events = createPageEvents(
    (onEvent) =>
      new Promise<void>((resolve) => {
        send = onEvent;
        open = resolve;
      }),
  );
  return {
    events,
    send: (e: unknown) => {
      send(e);
    },
    open: () => {
      open();
    },
  };
}

describe("updates", () => {
  it("takes the view now, then each change, and a change beats a late first answer", async () => {
    const stream = fakeStream();
    let answer: (v: unknown) => void = () => undefined;
    const ask = vi.fn(() => new Promise((resolve) => (answer = resolve)));
    const seen: (string | null)[] = [];
    const stop = watchUpdates((v) => seen.push(v.dot), stream.events, ask);
    stream.open();
    await Promise.resolve();
    stream.send({ kind: "updates", updates: view("Otaq 0.2.0 is ready") });
    answer(view(null));
    await new Promise((r) => setTimeout(r, 0));
    expect(ask).toHaveBeenCalledWith("updates.view");
    expect(seen).toEqual(["Otaq 0.2.0 is ready"]);
    stop();
  });

  it("sends each click and keeps the answer", async () => {
    const store = createStore<UpdatesView | undefined>(undefined);
    const ask = vi.fn((method: string) => Promise.resolve(method === "updates.open" ? null : view(method)));
    await updatesClick({ kind: "auto", on: false }, store, ask);
    expect(ask).toHaveBeenLastCalledWith("updates.auto", { on: false });
    expect(store.get()?.dot).toBe("updates.auto");
    await updatesClick({ kind: "skip" }, store, ask);
    expect(store.get()?.dot).toBe("updates.skip");
    await updatesClick({ kind: "restart" }, store, ask);
    expect(store.get()?.dot).toBe("updates.restart");
    await updatesClick({ kind: "open", what: "whats_new" }, store, ask);
    expect(ask).toHaveBeenLastCalledWith("updates.open", { what: "whats_new" });
    expect(store.get()?.dot).toBe("updates.restart");
  });

  it("draws the notice and the rows as the view says", () => {
    const notice = renderToStaticMarkup(
      createElement(UpdateNoticeBox, {
        notice: {
          kind: "needs_agents_restarted",
          version: "0.3.0",
          title: "Otaq 0.3.0 is ready · needs agents restarted (3 working)",
          body: "b",
          whats_new_label: "What’s new",
          notes_url: "https://example.com",
          skip_label: "Skip this version",
          action: "restart",
          action_label: "Restart now · stops 3 agents",
          primary: false,
          enabled: false,
        },
      }),
    );
    expect(notice).toContain('data-tone="paused"');
    expect(notice).toContain("Restart now · stops 3 agents");
    expect(notice).toMatch(/<button[^>]*disabled[^>]*>.*Restart now · stops 3 agents/);

    const rows = renderToStaticMarkup(
      createElement(UpdateRows, {
        view: {
          ...view(null),
          row: {
            label: "Updates",
            plain: null,
            check: { label: "Check for updates", enabled: false },
            result: {
              icon: "download",
              tone: "plain",
              text: "Downloading Otaq 0.2.0…",
              detail: null,
              progress: { permille: 615, text: "32 of 52 MB" },
            },
          },
        },
      }),
    );
    expect(rows).toContain('role="switch"');
    expect(rows).toContain("width:61.5%");
    expect(rows).toContain("32 of 52 MB");
  });
});
