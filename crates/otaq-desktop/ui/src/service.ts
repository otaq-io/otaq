// The page's side of the service link (T-0211): one event stream shared by every screen, a view that loads once and
// refetches on named events, and the connection state. Screens use the hooks; the plain functions under them hold the
// logic and are what the tests drive.
import { type Dispatch, type SetStateAction, useEffect, useState } from "react";

import type { PageEvent } from "./generated/PageEvent";
import type { RequestError } from "./generated/RequestError";
import type { ServiceConnection } from "./generated/ServiceConnection";
import { request, subscribeEvents } from "./tauri";

type Listener = (event: PageEvent) => void;

export interface PageEvents {
  /** Adds a listener. `ready` resolves once the stream is open, so a load started after it misses no event. */
  listen(listener: Listener): { ready: Promise<void>; stop: () => void };
}

/** The page's event stream, opened once by the first listener and shared by all of them. */
export function createPageEvents(subscribe: typeof subscribeEvents = subscribeEvents): PageEvents {
  const listeners = new Set<Listener>();
  let opened: Promise<void> | undefined;
  return {
    listen(listener) {
      listeners.add(listener);
      opened ??= subscribe((event) => {
        for (const l of listeners) l(event as PageEvent);
      });
      return { ready: opened, stop: () => listeners.delete(listener) };
    },
  };
}

export const pageEvents = createPageEvents();

/** A rejected request as the page shows it: the Rust side's `RequestError`, or the text of any other failure. */
export function asRequestError(error: unknown): RequestError {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error as RequestError;
  }
  return { message: String(error), refusal: null, not_found: false };
}

export interface ViewState<T> {
  /** The last view loaded; kept while a refetch fails. */
  view?: T;
  /** Why the last load failed, until one succeeds. */
  error?: RequestError;
}

/**
 * Loads a view once the event stream is open, then again on each event named in `refetchOn` (the service's event
 * kinds, e.g. `ticket_changed`) and on every resync. One load at a time: events that come while one is out ask for
 * one more load once it answers, and every answer is shown. (Dropping an answer because a newer load had started
 * starved a view while events kept coming: every view's loads wait in line on the one link to the service, so a
 * working agent's stream of events kept the board's card a step behind the sheet's pill, or the pill behind the
 * card, for seconds.) Returns the stop.
 */
export function watchView<T>(
  load: () => Promise<T>,
  refetchOn: readonly string[],
  onChange: (state: ViewState<T>) => void,
  events: PageEvents = pageEvents,
): () => void {
  let stopped = false;
  let loading = false;
  let again = false;
  let view: T | undefined;
  const fetch = () => {
    if (stopped) return;
    if (loading) {
      again = true;
      return;
    }
    loading = true;
    const done = () => {
      loading = false;
      if (again) {
        again = false;
        fetch();
      }
    };
    load().then(
      (loaded) => {
        if (stopped) return;
        view = loaded;
        onChange({ view });
        done();
      },
      (error: unknown) => {
        if (stopped) return;
        onChange({ view, error: asRequestError(error) });
        done();
      },
    );
  };
  const listening = events.listen((event) => {
    if (event.kind === "resync" || (event.kind === "service" && refetchOn.includes(event.name))) fetch();
  });
  listening.ready.then(fetch, (error: unknown) => {
    if (!stopped) onChange({ error: asRequestError(error) });
  });
  return () => {
    stopped = true;
    listening.stop();
  };
}

/** Follows the link to the service: its state now (`service.connection`), then each change. Returns the stop. */
export function watchConnection(
  onChange: (connection: ServiceConnection) => void,
  events: PageEvents = pageEvents,
): () => void {
  let stopped = false;
  let changed = false;
  const listening = events.listen((event) => {
    if (event.kind !== "connection" || stopped) return;
    changed = true;
    onChange(event.connection);
  });
  listening.ready
    .then(() => request("service.connection"))
    .then(
      (connection) => {
        // A change that arrived meanwhile is newer than this answer.
        if (!stopped && !changed) onChange(connection as ServiceConnection);
      },
      (error: unknown) => {
        if (!stopped) onChange({ state: "failed", reason: asRequestError(error).message });
      },
    );
  return () => {
    stopped = true;
    listening.stop();
  };
}

/**
 * A screen's view: loaded, and refetched on the named events and on resyncs. `load` must keep its identity between
 * renders (`useCallback`), or the view reloads on every render.
 */
export function useServiceView<T>(load: () => Promise<T>, refetchOn: readonly string[]): ViewState<T> {
  const [state, setState] = useState<ViewState<T>>({});
  const names = refetchOn.join(" ");
  useEffect(() => watchView(load, names.split(" "), setState), [load, names]);
  return state;
}

/**
 * Bumps `tick` every second while `on`: a view that shows a time in seconds (a running hook's) loads again with it.
 */
export function useEverySecond(on: boolean, tick: Dispatch<SetStateAction<number>>): void {
  useEffect(() => {
    if (!on) return;
    const timer = setInterval(() => {
      tick((t) => t + 1);
    }, 1_000);
    return () => {
      clearInterval(timer);
    };
  }, [on, tick]);
}

/** The link to the service; `undefined` until its state is known. */
export function useServiceConnection(): ServiceConnection | undefined {
  const [connection, setConnection] = useState<ServiceConnection>();
  useEffect(() => watchConnection(setConnection), []);
  return connection;
}
