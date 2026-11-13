import { beforeEach, describe, expect, it, vi } from "vitest";

import type { PageEvent } from "./generated/PageEvent";
import type { ServiceConnection } from "./generated/ServiceConnection";
import { asRequestError, createPageEvents, watchConnection, watchView } from "./service";
import { request } from "./tauri";

vi.mock("./tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn() }));
const requestMock = vi.mocked(request);

/** An event stream the test feeds by hand; `open()` resolves the subscription. */
function fakeStream() {
  let send: (event: unknown) => void = () => undefined;
  let open: () => void = () => undefined;
  const subscribe = vi.fn((onEvent: (event: unknown) => void) => {
    send = onEvent;
    return new Promise<void>((resolve) => (open = resolve));
  });
  const events = createPageEvents(subscribe);
  return {
    events,
    subscribe,
    open: () => {
      open();
    },
    send: (event: PageEvent) => {
      send(event);
    },
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const status = (pid: number) => ({ pid, build_id: "abc.1", agents_working: 0, agents_waiting: 0 });

beforeEach(() => {
  requestMock.mockReset();
});

describe("the event stream", () => {
  it("opens once and reaches every listener until it stops", () => {
    const stream = fakeStream();
    const first: PageEvent[] = [];
    const second: PageEvent[] = [];
    const a = stream.events.listen((e) => first.push(e));
    stream.events.listen((e) => second.push(e));
    expect(stream.subscribe).toHaveBeenCalledTimes(1);
    stream.send({ kind: "resync" });
    a.stop();
    stream.send({ kind: "resync" });
    expect(first).toHaveLength(1);
    expect(second).toHaveLength(2);
  });
});

describe("a view", () => {
  it("loads through the request table after subscribing, and refetches on its named events", async () => {
    const stream = fakeStream();
    requestMock
      .mockResolvedValueOnce(status(1))
      .mockResolvedValueOnce(status(2))
      .mockResolvedValueOnce(status(3));
    const seen: unknown[] = [];
    const stop = watchView(
      () => request("service.status"),
      ["projects_changed"],
      (state) => seen.push(state.view),
      stream.events,
    );
    await settle();
    expect(requestMock).not.toHaveBeenCalled();
    stream.open();
    await settle();
    expect(requestMock).toHaveBeenCalledWith("service.status");
    expect(seen).toEqual([status(1)]);

    stream.send({ kind: "service", name: "ticket_changed", event: { kind: "ticket_changed" } });
    await settle();
    expect(seen).toHaveLength(1);

    stream.send({ kind: "service", name: "projects_changed", event: { kind: "projects_changed" } });
    await settle();
    stream.send({ kind: "resync" });
    await settle();
    expect(seen).toEqual([status(1), status(2), status(3)]);

    stop();
    stream.send({ kind: "resync" });
    await settle();
    expect(requestMock).toHaveBeenCalledTimes(3);
  });

  it("keeps the last view when a refetch fails, and loads once more for the events that came during a load", async () => {
    const stream = fakeStream();
    let answerSlow: (value: string) => void = () => undefined;
    const slow = new Promise<string>((resolve) => (answerSlow = resolve));
    const answers = [
      () => Promise.resolve("first"),
      () => Promise.reject(new Error("no ticket")),
      () => slow,
      () => Promise.resolve("newest"),
    ];
    let calls = 0;
    const load = () => answers[calls++]?.() ?? Promise.reject(new Error("too many loads"));
    const states: unknown[] = [];
    watchView(load, ["x"], (state) => states.push(state), stream.events);
    stream.open();
    await settle();
    stream.send({ kind: "resync" });
    await settle();
    // Two events while the slow load is out: one more load after it, not two.
    stream.send({ kind: "resync" });
    stream.send({ kind: "resync" });
    stream.send({ kind: "resync" });
    await settle();
    expect(calls).toBe(3);
    answerSlow("slow");
    await settle();
    expect(calls).toBe(4);
    expect(states).toEqual([
      { view: "first" },
      { view: "first", error: expect.objectContaining({ message: "no ticket" }) as unknown },
      { view: "slow" },
      { view: "newest" },
    ]);
  });

  it("never falls behind while events keep coming: the card and the sheet's pill show the same update", async () => {
    const stream = fakeStream();
    // The one link to the service: it answers one request at a time, in order, reading the ticket as it answers.
    let status = "Working";
    const line: (() => void)[] = [];
    const link = (view: string) =>
      new Promise<string>((resolve) => {
        line.push(() => {
          resolve(`${view} ${status}`);
        });
      });
    const answerOne = async () => {
      line.shift()?.();
      await settle();
    };
    const card: unknown[] = [];
    const pill: unknown[] = [];
    watchView(
      () => link("card"),
      ["ticket_changed"],
      (s) => card.push(s.view),
      stream.events,
    );
    watchView(
      () => link("pill"),
      ["ticket_changed"],
      (s) => pill.push(s.view),
      stream.events,
    );
    stream.open();
    await settle();
    // An agent at work: another event comes before each answer.
    const changed = () => {
      stream.send({ kind: "service", name: "ticket_changed", event: { kind: "ticket_changed" } });
    };
    for (let i = 0; i < 4; i++) {
      if (i === 2) status = "Idle";
      changed();
      await answerOne();
    }
    expect(card.at(-1)).toBe("card Idle");
    expect(pill.at(-1)).toBe("pill Idle");
    // The line to the service stays short: one load out per view, whatever the events.
    expect(line.length).toBeLessThanOrEqual(2);
  });
});

describe("the connection", () => {
  it("starts from service.connection and follows the connection events", async () => {
    const stream = fakeStream();
    requestMock.mockResolvedValue({ state: "connecting" });
    const seen: ServiceConnection[] = [];
    watchConnection((c) => seen.push(c), stream.events);
    stream.open();
    await settle();
    expect(requestMock).toHaveBeenCalledWith("service.connection");
    stream.send({ kind: "connection", connection: { state: "failed", reason: "path too long" } });
    expect(seen).toEqual([{ state: "connecting" }, { state: "failed", reason: "path too long" }]);
  });

  it("drops the first answer when a change arrived before it", async () => {
    const stream = fakeStream();
    let answer: (value: unknown) => void = () => undefined;
    requestMock.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    const seen: ServiceConnection[] = [];
    watchConnection((c) => seen.push(c), stream.events);
    stream.open();
    await settle();
    stream.send({ kind: "connection", connection: { state: "stopped" } });
    answer({ state: "connecting" });
    await settle();
    expect(seen).toEqual([{ state: "stopped" }]);
  });
});

describe("request errors", () => {
  it("keep the Rust side's error and wrap anything else", () => {
    const error = { message: "cycle", refusal: "cycle", not_found: false };
    expect(asRequestError(error)).toBe(error);
    expect(asRequestError("command not found")).toEqual({
      message: "command not found",
      refusal: null,
      not_found: false,
    });
  });
});
