import { describe, expect, it, vi } from "vitest";

import type { Keymap } from "../generated/Keymap";
import type { PageEvent } from "../generated/PageEvent";
import { createPageEvents } from "../service";
import { request } from "../tauri";
import { createSequenceWait, SEQUENCE_WAIT_MS, watchKeymap } from "./keyboard";
import { createStore } from "./store";

vi.mock("../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn() }));
const requestMock = vi.mocked(request);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const map = (letters: boolean): Keymap => ({
  platform: "mac",
  bindings: [],
  letters,
  actions: [],
  sequences: [],
  terminal_app_modifiers: [],
  terminal_keys: [],
  log: false,
  typing_hint: "Typing? Click where you want to type",
});

describe("the keyboard map", () => {
  it("is asked once the events are open, and again on each settings change (d0097)", async () => {
    let send: (event: PageEvent) => void = () => undefined;
    const events = createPageEvents((onEvent) => {
      send = onEvent;
      return Promise.resolve();
    });
    requestMock.mockResolvedValueOnce(map(true)).mockResolvedValueOnce(map(false));
    const seen: boolean[] = [];
    const stop = watchKeymap((m) => seen.push(m.letters), events);
    await settle();
    expect(requestMock).toHaveBeenLastCalledWith("keymap.map");

    send({ kind: "service", name: "ticket_changed", event: {} });
    send({ kind: "service", name: "settings_changed", event: {} });
    await settle();
    expect(requestMock).toHaveBeenCalledTimes(2);
    expect(seen).toEqual([true, false]);
    stop();
  });
});

describe("the wait after g", () => {
  it("ends by itself after about a second, or when ended; starting again starts the second anew", () => {
    vi.useFakeTimers();
    const store = createStore(false);
    const wait = createSequenceWait(store, globalThis);
    wait(true);
    expect(store.get()).toBe(true);
    vi.advanceTimersByTime(SEQUENCE_WAIT_MS - 1);
    expect(store.get()).toBe(true);
    vi.advanceTimersByTime(1);
    expect(store.get()).toBe(false);
    wait(true);
    vi.advanceTimersByTime(SEQUENCE_WAIT_MS / 2);
    wait(true);
    vi.advanceTimersByTime(SEQUENCE_WAIT_MS / 2);
    expect(store.get()).toBe(true);
    wait(false);
    expect(store.get()).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
  });
});
