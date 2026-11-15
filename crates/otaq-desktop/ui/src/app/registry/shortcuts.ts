// The shortcut handlers: `KeyAction` → handler, filled by screens. The key map and the dispatcher that turns a key
// into a `KeyAction` are T-0215's (the Rust keymap stays the only map); it calls `run`.
//
// ⌘K and ⌘B in the description: Link and Bold. While the description editor has focus, ⌘K / Ctrl+K never reaches the
// `command_palette` handlers, nor ⌘B / Ctrl+B whatever action the user recorded on it: the dispatcher leaves them to
// the editor (`keysKeptBy`, d0078 D2). The comment box's text keeps ⌘B, ⌘I, ⌘E and ⌘K the same way, for its Bold,
// Italic, Code and Link (tuq 111). Everywhere else ⌘K is the palette (d0097; the base map has no ⌘B since d0113).
import type { KeyAction } from "../../generated/KeyAction";

export type KeyActionName = KeyAction["action"];

/** The action of that name, with its number for `project` and `switch_tab`. */
export type ActionNamed<N extends KeyActionName> = Extract<KeyAction, { action: N }>;

/** Returns `false` to leave the key to the handler registered before it; anything else handles it. */
export type ShortcutHandler<N extends KeyActionName> = (action: ActionNamed<N>) => unknown;

export interface Shortcuts {
  /** Adds a handler, tried before those added earlier. Returns its removal (for a component's effect). */
  on<N extends KeyActionName>(name: N, handler: ShortcutHandler<N>): () => void;
  /** Runs the newest handler for `action` that takes it. Whether one did. */
  run(action: KeyAction): boolean;
  /** The actions with at least one handler. */
  names(): KeyActionName[];
  /** Calls `listener` after each action a handler took, from a key or the palette (its recent items, T-0393). Returns
   * its removal. */
  onRan(listener: (action: KeyAction) => void): () => void;
}

export function createShortcuts(): Shortcuts {
  const handlers = new Map<KeyActionName, ((action: KeyAction) => unknown)[]>();
  const ran = new Set<(action: KeyAction) => void>();
  return {
    on(name, handler) {
      const entry = handler as (action: KeyAction) => unknown;
      handlers.set(name, [...(handlers.get(name) ?? []), entry]);
      return () => {
        const rest = (handlers.get(name) ?? []).filter((h) => h !== entry);
        if (rest.length > 0) handlers.set(name, rest);
        else handlers.delete(name);
      };
    },
    run(action) {
      const list = handlers.get(action.action) ?? [];
      const taken = list.toReversed().some((handler) => handler(action) !== false);
      if (taken) for (const listener of ran) listener(action);
      return taken;
    },
    names: () => [...handlers.keys()],
    onRan(listener) {
      ran.add(listener);
      return () => {
        ran.delete(listener);
      };
    },
  };
}
