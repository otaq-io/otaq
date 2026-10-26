import { afterEach, describe, expect, it, vi } from "vitest";

import type { LayoutView } from "../generated/LayoutView";
import { createPageEvents } from "../service";
import { request } from "../tauri";
import { layoutView, loadLayout } from "./layout";

vi.mock("../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn() }));
const requestMock = vi.mocked(request);

const VIEW = { rail: 64, agent_panel: 420 } as LayoutView;

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(() => {
  requestMock.mockReset();
  layoutView.set(undefined);
  vi.restoreAllMocks();
});

describe("the layout", () => {
  it("asked before the service is connected, loads on the resync after the connect", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    let send: (event: unknown) => void = () => undefined;
    const events = createPageEvents((onEvent) => {
      send = onEvent;
      return Promise.resolve();
    });
    requestMock.mockRejectedValueOnce({ message: "not connected", refusal: null, not_found: false });
    const stop = loadLayout(events);
    await settle();
    expect(layoutView.get()).toBeUndefined();

    requestMock.mockResolvedValueOnce(VIEW);
    send({ kind: "resync" });
    await settle();
    expect(layoutView.get()).toEqual(VIEW);
    stop();
  });
});
