// The dispatcher on the page (T-0215): one capturing `keydown` listener on the window, so it sees each key before the
// focused element (a terminal's xterm, a text field) does. The decisions are `keys.ts`'s. The map is the user's keys,
// asked again on every `settings_changed` (d0097: a key changed in Settings applies at once). After `g` it waits about a
// second for the second key (`sequenceWait`, which the hint shows). A fast burst of letters with nothing focused runs
// nothing (d0126): what its first letter opened closes again and the typing hint shows (`typingHint`).
import type { Keymap } from "../generated/Keymap";
import { type PageEvents, pageEvents, watchView } from "../service";
import { request } from "../tauri";
import {
  type Burst,
  createBurst,
  dispatchKey,
  inEditorText,
  inKeyRecorder,
  isModifierKey,
  keysKeptBy,
  type LetterHold,
  lettersKeptBy,
  logLine,
  stopsEvent,
  widgetOpen,
} from "./keys";
import { modalOpen, type OverlayStack, overlays } from "./overlays";
import type { Registries } from "./registry";
import { focusTerminal, selection } from "./selection";
import { createStore } from "./store";

/** The user's keys from Rust (§21.3); `undefined` until they arrived. */
export const keymap = createStore<Keymap | undefined>(undefined);

/** Follows the map: once the event stream is open, then on each settings change. Returns the stop. */
export function watchKeymap(onChange: (map: Keymap) => void, events: PageEvents = pageEvents): () => void {
  return watchView(
    () => request("keymap.map") as Promise<Keymap>,
    ["settings_changed"],
    (state) => {
      if (state.view) onChange(state.view);
      else if (state.error) console.error("the keyboard map couldn't load", state.error);
    },
    events,
  );
}

/** Whether Otaq waits for a `g` sequence's second key (KEY-02): the hint shows meanwhile. */
export const sequenceWait = createStore(false);

/** How long Otaq waits after `g` for the second key, in ms ("about a second", d0097). */
export const SEQUENCE_WAIT_MS = 1000;

/** Starts or ends the wait after `g`; it ends by itself after `SEQUENCE_WAIT_MS`. */
export function createSequenceWait(
  store = sequenceWait,
  timers: Pick<Window, "setTimeout" | "clearTimeout"> = window,
): (on: boolean) => void {
  let timer: number | undefined;
  return (on) => {
    if (timer !== undefined) timers.clearTimeout(timer);
    timer = undefined;
    if (on)
      timer = timers.setTimeout(() => {
        timer = undefined;
        store.set(false);
      }, SEQUENCE_WAIT_MS);
    store.set(on);
  };
}

/** Whether the typing hint shows ("Typing? Click where you want to type", d0126). */
export const typingHint = createStore(false);

/** How long the typing hint stays after the burst's last key, in ms. */
export const TYPING_HINT_MS = 2500;

/** A menu's Esc sent by the guard's undo: the dispatcher leaves it alone. */
let undoing = false;

/** What the page's burst guard reads and closes: the overlays, the focus, the `g` wait, the hint, the clock. */
export interface BurstPage {
  overlays: OverlayStack;
  /** The focused element now. */
  focused: () => Element | null;
  wait: (on: boolean) => void;
  hint: typeof typingHint;
  now: () => number;
  timers: Pick<Window, "setTimeout" | "clearTimeout">;
}

/**
 * The page's burst guard (d0126): before a letter runs it notes the open overlays and the focus; when a burst begins it
 * closes the overlays opened since (New ticket, the `?` list), a menu the letter opened (its own Esc), the `g` wait, and
 * gives the focus back, then shows the hint until the typing has paused a moment.
 */
export function createPageBurst(page: BurstPage): Burst & { stop: () => void } {
  let before: { open: ReadonlySet<string>; focused: Element | null } | null = null;
  let timer: number | undefined;
  const hideLater = () => {
    if (timer !== undefined) page.timers.clearTimeout(timer);
    timer = page.timers.setTimeout(() => {
      timer = undefined;
      page.hint.set(false);
    }, TYPING_HINT_MS);
  };
  const undo = () => {
    page.wait(false);
    if (before === null) return;
    const { open, focused } = before;
    before = null;
    for (const o of page.overlays.store.get()) if (!open.has(o.id)) page.overlays.close(o.id);
    const now = page.focused();
    if (now === focused || !(now instanceof HTMLElement)) return;
    if (now.closest('[role="menu"]')) {
      now.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
    }
    if (focused instanceof HTMLElement && focused !== now.ownerDocument.body && focused.isConnected)
      focused.focus();
    else now.blur();
  };
  const burst = createBurst({
    now: page.now,
    note: () => {
      before = { open: new Set(page.overlays.store.get().map((o) => o.id)), focused: page.focused() };
    },
    dropped: (first) => {
      if (first) {
        undoing = true;
        try {
          undo();
        } finally {
          undoing = false;
        }
      }
      page.hint.set(true);
      hideLater();
    },
  });
  return Object.assign(burst, {
    stop: () => {
      if (timer !== undefined) page.timers.clearTimeout(timer);
      page.hint.set(false);
    },
  });
}

/** The newest key log lines, newest last (only filled with `OTAQ_KEY_LOG=1`). */
export const keyLog = createStore<readonly string[]>([]);

const LOG_LINES = 12;

/** The letters the page keeps from the app now: all of them while a menu or dialog is open, else the focus's. */
function lettersHere(focused: HTMLElement | null): LetterHold {
  return widgetOpen(document) ? "all" : lettersKeptBy(focused);
}

/** Loads the map and starts dispatching every key of `target`. Returns the stop. */
export function installKeys(app: Registries, target: Window = window): () => void {
  const stopWatching = watchKeymap(keymap.set);
  const wait = createSequenceWait();
  const burst = createPageBurst({
    overlays,
    focused: () => document.activeElement,
    wait,
    hint: typingHint,
    now: () => Date.now(),
    timers: window,
  });
  const onKey = (event: KeyboardEvent) => {
    if (isModifierKey(event) || event.isComposing || undoing) return;
    const focused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const outcome = dispatchKey(
      {
        keymap: keymap.get,
        run: (action) => app.shortcuts.run(action),
        leaveTerminal: () => {
          focusTerminal(null);
          focused?.blur();
        },
        mac: navigator.userAgent.includes("Mac"),
        waiting: sequenceWait.get,
        wait,
        burst,
      },
      event,
      {
        inTerminal: selection.get().focusedTerminal !== null,
        focusKeeps: keysKeptBy(focused),
        modal: modalOpen(overlays.store.get()),
        editorText: inEditorText(focused),
        letters: lettersHere(focused),
        recording: inKeyRecorder(focused),
      },
    );
    if (stopsEvent(outcome)) {
      event.preventDefault();
      event.stopPropagation();
    }
    if (keymap.get()?.log === true) {
      const line = logLine(event, outcome);
      console.info(`key: ${line}`);
      keyLog.update((lines) => [...lines, line].slice(-LOG_LINES));
    }
  };
  target.addEventListener("keydown", onKey, true);
  return () => {
    stopWatching();
    wait(false);
    burst.stop();
    target.removeEventListener("keydown", onKey, true);
  };
}
