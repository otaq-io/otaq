// Self-updates on the page (UPD-04, d0140; page 32 items 1–2): About's switch, Updates row and notice, and the dot on
// Settings in the rail. Every text and state comes from Rust (`updates.view`, then each change as an `updates`
// event); this file only keeps the last view and sends the clicks.
import { createStore, type Store } from "../../app/store";
import type { UpdatesView } from "../../generated/UpdatesView";
import type { UpdatesOpen } from "../../generated/UpdatesOpen";
import { type PageEvents, pageEvents } from "../../service";
import { request } from "../../tauri";

/** The update view as last heard; `undefined` until the first answer. */
export const updatesStore: Store<UpdatesView | undefined> = createStore<UpdatesView | undefined>(undefined);

/** Follows the update view: now (`updates.view`), then each change. Returns the stop. */
export function watchUpdates(
  onChange: (view: UpdatesView) => void,
  events: PageEvents = pageEvents,
  ask: typeof request = request,
): () => void {
  let stopped = false;
  let changed = false;
  const listening = events.listen((event) => {
    if (event.kind !== "updates" || stopped) return;
    changed = true;
    onChange(event.updates);
  });
  listening.ready
    .then(() => ask("updates.view"))
    .then(
      (view) => {
        // A change that arrived meanwhile is newer than this answer.
        if (!stopped && !changed) onChange(view as UpdatesView);
      },
      () => undefined,
    );
  return () => {
    stopped = true;
    listening.stop();
  };
}

/** What a click in About's update parts asks; each answers with the view, which goes to the store. */
export type UpdatesClick =
  | { kind: "check" }
  | { kind: "auto"; on: boolean }
  | { kind: "skip" }
  | { kind: "restart" }
  | { kind: "open"; what: UpdatesOpen };

export async function updatesClick(
  click: UpdatesClick,
  store: Store<UpdatesView | undefined> = updatesStore,
  ask: typeof request = request,
): Promise<void> {
  switch (click.kind) {
    case "check":
      store.set((await ask("updates.check")) as UpdatesView);
      return;
    case "auto":
      store.set((await ask("updates.auto", { on: click.on })) as UpdatesView);
      return;
    case "skip":
      store.set((await ask("updates.skip")) as UpdatesView);
      return;
    // Puts the update in place and reopens the app; answers only when that failed (About then says why).
    case "restart":
      store.set((await ask("updates.restart")) as UpdatesView);
      return;
    case "open":
      await ask("updates.open", { what: click.what });
      return;
  }
}
