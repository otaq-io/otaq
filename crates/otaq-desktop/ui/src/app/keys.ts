// The page's one key dispatcher (T-0215, SES-10, UIT-06). The map is the Rust keymap (`keymap.map`, the user's keys
// of d0097's §21.3 map), so nothing here lists a shortcut: a key either goes to the focused terminal, or runs the
// screen handler of the action it is bound to (`app.shortcuts.run`), or, if it is a key the webview would act on
// (reload, print, find, zoom, devtools, caret browsing, history), is swallowed without doing anything. Single letters
// and `g` sequences (KEY-02) run only while nobody is typing and the letters switch is on; after `g` the dispatcher
// waits for the second key (`Dispatch.wait`, whose timer and hint are `keyboard.ts`'s). Letters typed faster than about
// 3 a second with nothing focused are typing, not shortcuts (d0126): from the second one until a pause none runs, and
// what the first one opened is closed again (`Burst`).
import type { KeyAction } from "../generated/KeyAction";
import type { KeyBinding } from "../generated/KeyBinding";
import type { Keymap } from "../generated/Keymap";
import type { KeyModifiers } from "../generated/KeyModifiers";
import type { Keystroke } from "../generated/Keystroke";

/** What the dispatcher reads of a key event; a `KeyboardEvent` is one. */
export interface KeyLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  /** The key is held down and this is its auto-repeat. */
  repeat?: boolean;
}

/** Where the key was pressed. */
export interface KeyContext {
  /** A terminal has the keyboard (the selection's `focusedTerminal`). */
  inTerminal: boolean;
  /** The keys the focused control acts on itself (`keysKeptBy`); a binding on one of them waits. */
  focusKeeps: ReadonlySet<string>;
  /** A modal overlay is open: the app's shortcuts don't run (`modalOpen`). */
  modal: boolean;
  /** The description editor's text has the focus (`inEditorText`): ⌥⌘C is its Code block, not swallowed. */
  editorText: boolean;
  /** The single letters the focus keeps from the app (`lettersKeptBy`; `"all"` while a menu or dialog is open). */
  letters: LetterHold;
  /** A Settings shortcut cell is recording (`KEY_RECORDER`): every key is the cell's. */
  recording: boolean;
}

/** The element a Settings shortcut cell marks while it records a key (T-0395): the dispatcher leaves every key to it. */
export const KEY_RECORDER = "[data-key-recorder]";

/** Whether the focus is in a recording shortcut cell. */
export function inKeyRecorder(element: FocusLike | null): boolean {
  return Boolean(element?.closest?.(KEY_RECORDER));
}

/**
 * Which single letters stay with the focused element (KEY-02, "nobody is typing"): all of them while somebody types
 * or a menu or dialog has the keyboard; the digits in a radio group (an answer's 1–4); none elsewhere.
 */
export type LetterHold = "all" | "digits" | "none";

/** What became of a key. */
export type KeyOutcome =
  /** Left to the focused terminal (the session gets it). */
  | { kind: "terminal" }
  /** Its action's handler took it. */
  | { kind: "ran"; action: KeyAction }
  /** Bound, but no handler took it (or a modal held it back); swallowed outside a terminal. */
  | { kind: "unhandled"; action: KeyAction; swallowed: boolean }
  /** `g`: Otaq waits for a sequence's second key. */
  | { kind: "waiting" }
  /** Pressed while Otaq waited after `g` and no sequence's: the wait ends and the key does nothing more. */
  | { kind: "cancelled" }
  /** Typed in a fast burst with nothing focused (d0126): it runs nothing and is dropped. */
  | { kind: "burst" }
  /** Not in the map, but the webview would act on it: swallowed. */
  | { kind: "swallowed" }
  /** Neither: the page's elements get it as usual. */
  | { kind: "passed" };

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "OS", "AltGraph", "CapsLock", "Fn"]);

const TEXT_FIELD_KEYS: ReadonlySet<string> = new Set(["Enter", "Backspace", "Escape"]);
const CONTROL_KEYS: ReadonlySet<string> = new Set(["Enter"]);
const NO_KEYS: ReadonlySet<string> = new Set();
const CONTROL_ROLES = new Set(["button", "link", "menuitem", "option", "tab", "checkbox", "radio", "switch"]);
/** Inside these, ↵ and Esc are the widget's (T-0217): a menu chooses and closes; a dialog or popover presses its
 * default button and cancels (`dialog.rs`). */
const WIDGETS = '[role="menu"], [role="dialog"]';
const WIDGET_KEYS: ReadonlySet<string> = new Set(["Enter", "Escape"]);
/** The description editor (d0078 D2, d0097): while its text or one of its fields (the link popover's URL)
 * has the focus, ⌘K / Ctrl+K is its Link, not the command palette, and ⌘B / Ctrl+B its Bold, whatever the user
 * recorded on it (`description_editor_keys`). Its read mode isn't a text field and keeps nothing. */
const DESCRIPTION_EDITOR = ".md-editor";
const EDITOR_KEYS: ReadonlySet<string> = new Set([...TEXT_FIELD_KEYS, "k", "b"]);
/** Where the app's single letters don't run (KEY-02): a menu, a list or a dialog has its own keys. */
const LETTER_WIDGETS = '[role="menu"], [role="listbox"], [role="dialog"]';
const RADIO_GROUP = '[role="radiogroup"]';
/** A menu or dialog open anywhere on the page holds the letters back too; a leaving copy (`data-leaving`) doesn't. */
const OPEN_WIDGETS = '[role="menu"], [role="dialog"]';

/** What `keysKeptBy` reads of the focused element; an `HTMLElement` is one. */
export interface FocusLike {
  tagName: string;
  isContentEditable: boolean;
  getAttribute: (name: string) => string | null;
  /** The nearest ancestor (or itself) matching `selector`, as `Element.closest`. */
  closest?: (selector: string) => unknown;
}

/**
 * The keys the focused element acts on itself, so a binding on them waits: a text field keeps Enter, Backspace and
 * Esc (↵ Open panel, ⌘⌫ Send back and Esc Close panel don't fire while typing); a button, link or list keeps Enter;
 * anything in a menu, dialog or popover also keeps Enter and Esc. A text field of the description editor also keeps
 * K, so ⌘K reaches the editor's Link (d0078 D2); everywhere else ⌘K stays the command palette.
 */
export function keysKeptBy(element: FocusLike | null): ReadonlySet<string> {
  let own = keptByElement(element);
  if (own === TEXT_FIELD_KEYS && element?.closest?.(DESCRIPTION_EDITOR)) own = EDITOR_KEYS;
  // The editor's field itself, focused after Esc left its text (T-0259): Esc then goes to the form (discard), not
  // the panel, and ↵ goes back into the text.
  else if (own === NO_KEYS && element !== null && element.closest?.(DESCRIPTION_EDITOR) === element)
    own = TEXT_FIELD_KEYS;
  if (!element?.closest?.(WIDGETS)) return own;
  return new Set([...own, ...WIDGET_KEYS]);
}

/** Which single letters the focused element keeps (KEY-02). */
export function lettersKeptBy(element: FocusLike | null): LetterHold {
  if (element === null) return "none";
  if (keptByElement(element) === TEXT_FIELD_KEYS || isSelect(element)) return "all";
  if (element.closest?.(DESCRIPTION_EDITOR) || element.closest?.(LETTER_WIDGETS)) return "all";
  return element.closest?.(RADIO_GROUP) ? "digits" : "none";
}

const isSelect = (element: FocusLike) => element.tagName.toLowerCase() === "select";

/** What `widgetOpen` reads of the page; a `Document` is one. */
export interface PageLike {
  querySelectorAll: (selector: string) => Iterable<{ closest: (selector: string) => unknown }>;
}

/** Whether a menu or a dialog is open on the page (KEY-02: then no single letter runs). */
export function widgetOpen(page: PageLike): boolean {
  for (const el of page.querySelectorAll(OPEN_WIDGETS)) if (!el.closest("[data-leaving]")) return true;
  return false;
}

/**
 * Whether the focus is in the description editor's text (d0078, Q51): the one place ⌥⌘C / Ctrl+Alt+C is Code block.
 * Its link popover's URL, its Markdown source and its read mode don't toggle code blocks, so there ⌥⌘C stays swallowed.
 */
export function inEditorText(element: FocusLike | null): boolean {
  return element !== null && element.isContentEditable && Boolean(element.closest?.(DESCRIPTION_EDITOR));
}

/** ⌥⌘C on a Mac: Safari's inspector for the webview, Code block for the description editor (Q51). */
function isCodeBlockKey(e: KeyLike, mac: boolean): boolean {
  return mac && e.metaKey && e.altKey && !e.ctrlKey && !e.shiftKey && letterOf(e) === "c";
}

function keptByElement(element: FocusLike | null): ReadonlySet<string> {
  if (element === null) return NO_KEYS;
  const tag = element.tagName.toLowerCase();
  if (element.isContentEditable || tag === "textarea") return TEXT_FIELD_KEYS;
  if (tag === "input") {
    const type = (element.getAttribute("type") ?? "text").toLowerCase();
    return ["button", "submit", "reset", "checkbox", "radio", "file", "color", "range"].includes(type)
      ? CONTROL_KEYS
      : TEXT_FIELD_KEYS;
  }
  if (["button", "a", "select", "summary"].includes(tag)) return CONTROL_KEYS;
  return CONTROL_ROLES.has(element.getAttribute("role") ?? "") ? CONTROL_KEYS : NO_KEYS;
}

export function modifiersOf(e: KeyLike): KeyModifiers {
  return { control: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, platform: e.metaKey, function: false };
}

/** "Ctrl+Shift+K", "Meta+K", "F5": a key as the log shows it. */
export function keyName(e: KeyLike): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.metaKey) parts.push("Meta");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return `${parts.join("+")} (${e.code})`;
}

const isLatinLetter = (key: string) => /^[a-z]$/i.test(key);

/**
 * The letter a key types, or else its name: letters follow the keyboard layout (⌘K is K on a Dvorak keyboard too);
 * where the layout types no Latin letter, the physical key's.
 */
function letterOf(e: KeyLike): string {
  if (isLatinLetter(e.key)) return e.key.toLowerCase();
  const physical = /^Key([A-Z])$/.exec(e.code);
  return physical ? (physical[1] ?? "").toLowerCase() : e.key.toLowerCase();
}

/** Whether `e` is `stroke`: the same modifiers, and the same letter, named key or (digits and signs) physical key. */
export function strokeMatches(stroke: Keystroke, e: KeyLike): boolean {
  const m = modifiersOf(e);
  const want = stroke.modifiers;
  if (
    m.control !== want.control ||
    m.alt !== want.alt ||
    m.shift !== want.shift ||
    m.platform !== want.platform
  ) {
    return false;
  }
  if (stroke.key.length > 1) return e.key === stroke.key;
  if (isLatinLetter(stroke.key)) return letterOf(e) === stroke.key;
  return e.code === stroke.code;
}

/**
 * Whether `e` types `letter` (a single letter or a sequence's second key), as `keymap::letter_matches`: a Latin letter
 * by the layout's letter without Shift (`J` isn't `j`), any other character as typed, Shift or not (`?`).
 */
export function letterMatches(letter: string, e: KeyLike): boolean {
  if (e.ctrlKey || e.altKey || e.metaKey) return false;
  return /^[a-z]$/.test(letter) ? !e.shiftKey && letterOf(e) === letter : e.key === letter;
}

/** The modifier key a binding has, or `undefined` for a single letter or a `g` sequence. */
function strokeOf(binding: KeyBinding): Keystroke | undefined {
  return binding.key.kind === "stroke" ? binding.key.stroke : undefined;
}

/** Whether a binding works where the key was pressed (`KeyPlace`). */
function worksHere(binding: KeyBinding, inTerminal: boolean): boolean {
  return inTerminal ? binding.place !== "outside_terminals" : binding.place !== "inside_terminals";
}

/**
 * Whether the key goes to the focused terminal: `keymap::route`'s rule, from the map. A key reaches the app when it
 * holds one of the terminal app modifiers (⌘ on a Mac) or is a binding that works in terminals (a Mac's ⌃` and ⌃Tab).
 */
export function goesToTerminal(map: Keymap, e: KeyLike): boolean {
  const held = modifiersOf(e);
  const app = map.terminal_app_modifiers.some(
    (m) =>
      (!m.control || held.control) &&
      (!m.alt || held.alt) &&
      (!m.shift || held.shift) &&
      (!m.platform || held.platform),
  );
  return !app && bindingFor(map, e, true) === undefined;
}

/** Whether a focused element keeping `kept` takes `stroke` from the app: a kept letter only without Shift (the
 * editor's ⌘B is Bold, its ⇧⌘B stays Board, Q221). */
/** A key with no modifier held (↵, Esc): a single-letter key for the letters switch (KEY-02). */
function withoutModifiers(stroke: Keystroke): boolean {
  const m = stroke.modifiers;
  return !m.control && !m.alt && !m.shift && !m.platform && !m.function;
}

function keeps(kept: ReadonlySet<string>, stroke: Keystroke): boolean {
  return kept.has(stroke.key) && !(stroke.key.length === 1 && stroke.modifiers.shift);
}

/** The single letter `e` presses, outside a terminal (`hold`: the letters the focus keeps). */
export function letterFor(map: Keymap, e: KeyLike, hold: LetterHold): KeyBinding | undefined {
  if (hold === "all") return undefined;
  return map.bindings.find(
    (b) =>
      b.key.kind === "letter" &&
      !(hold === "digits" && /^[0-9]$/.test(b.key.letter)) &&
      letterMatches(b.key.letter, e),
  );
}

/**
 * The open ticket sheet's own actions (`KeyAction::in_sheet`): where they share a key with another action (`g a`, the
 * Activity tab and Activity, SHB-04), the sheet's is tried first and the other runs when no sheet takes it.
 */
const SHEET_FIRST: ReadonlySet<KeyAction["action"]> = new Set<KeyAction["action"]>(["switch_tab"]);

/** The `g` sequences whose second key `e` is, the sheet's first. */
export function sequencesFor(map: Keymap, e: KeyLike): KeyBinding[] {
  const all = map.bindings.filter((b) => b.key.kind === "sequence" && letterMatches(b.key.then, e));
  return [
    ...all.filter((b) => SHEET_FIRST.has(b.action.action)),
    ...all.filter((b) => !SHEET_FIRST.has(b.action.action)),
  ];
}

/** Whether `e` is the `g` that starts the sequences (when the map has any). */
function startsSequence(map: Keymap, e: KeyLike): boolean {
  return letterMatches("g", e) && map.bindings.some((b) => b.key.kind === "sequence");
}

/** The binding `e` presses where it was pressed (in a terminal only those that work there). */
export function bindingFor(map: Keymap, e: KeyLike, inTerminal: boolean): KeyBinding | undefined {
  return map.bindings.find((b) => {
    const stroke = strokeOf(b);
    return stroke !== undefined && worksHere(b, inTerminal) && strokeMatches(stroke, e);
  });
}

const WEBVIEW_FUNCTION_KEYS = new Set([
  "F1", // help
  "F3", // find next
  "F5", // reload
  "F7", // caret browsing
  "F11", // full screen
  "F12", // devtools
  "BrowserBack",
  "BrowserForward",
  "BrowserRefresh",
  "BrowserSearch",
]);

/**
 * Keys a webview acts on by itself (UIT-06): reload, print, find, zoom, devtools, caret browsing, history, save, open,
 * view source. The Edit keys (copy, paste, undo…) aren't among them: text fields need them.
 */
export function isWebviewKey(e: KeyLike, mac: boolean): boolean {
  if (WEBVIEW_FUNCTION_KEYS.has(e.key)) return true;
  const command = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  // ⌥← / ⌥→ move by word in a Mac text field; elsewhere Alt+← / → is back and forward.
  if (!mac && e.altKey && !e.ctrlKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) return true;
  if (!command) return false;
  const key = letterOf(e);
  // Devtools, hard reload, save as (⇧⌘I, ⇧⌘J, ⇧⌘C, ⇧⌘R, ⇧⌘S); ⌥⌘I / ⌥⌘C / ⌥⌘J / ⌥⌘U open Safari's inspector.
  if (e.shiftKey && !e.altKey && ["i", "j", "c", "r", "s"].includes(key)) return true;
  if (mac && e.altKey && ["i", "j", "c", "u"].includes(key)) return true;
  // Find, find next, reload, print, save, open, view source; zoom in, out and back.
  if (!e.altKey && ["f", "g", "r", "p", "s", "o", "u"].includes(key)) return true;
  return (
    ["=", "+", "-", "_", "0"].includes(e.key) ||
    ["Equal", "Minus", "Digit0", "NumpadAdd", "NumpadSubtract"].includes(e.code)
  );
}

export interface Dispatch {
  /** The map; `undefined` until `keymap.map` answered (keys then only get swallowed or passed). */
  keymap: () => Keymap | undefined;
  /** Runs the newest handler for the action; whether one took it (`app.shortcuts.run`). */
  run: (action: KeyAction) => boolean;
  /** Leaves the focused terminal: Terminals (⌃`) pressed in one. */
  leaveTerminal: () => void;
  /** Whether Otaq waits for a sequence's second key after `g`. */
  waiting: () => boolean;
  /** Starts the wait after `g` (its hint and its timer) or ends it. */
  wait: (on: boolean) => void;
  /** Whether this is a Mac (only used before the map arrived). */
  mac: boolean;
  /** The fast-typing guard (d0126); without one each key is judged alone. */
  burst?: Burst;
}

/** Letters this close together (ms) are typing, not shortcuts: about 3 a second (d0126). */
export const BURST_MS = 300;

/**
 * The fast-typing guard's state and what it asks of the page (d0126): when the previous typed key came while letters
 * ran, whether a burst is on, and the page's part, noting what a letter is about to open and closing it again.
 */
export interface Burst {
  /** The time in ms. */
  now: () => number;
  /** Before a typed key with nothing focused: what is open now, so what the key opens can be closed again. */
  note: () => void;
  /** A key of a burst was dropped; `first`: the burst began, so what the noted key opened closes and the hint shows. */
  dropped: (first: boolean) => void;
  /** When the previous typed key came while it counted (nothing focused, or right after one); `null`: none. */
  last: number | null;
  /** Typing is fast now: nothing runs until a pause. */
  on: boolean;
}

export function createBurst(page: Pick<Burst, "now" | "note" | "dropped">): Burst {
  return { ...page, last: null, on: false };
}

/** A key that types a character (Shift or not): Space and signs too, not with ⌘, Ctrl or ⌥. */
function typesCharacter(e: KeyLike): boolean {
  return !e.ctrlKey && !e.metaKey && !e.altKey && Array.from(e.key).length === 1;
}

/** Keys that are part of typing once a burst is on: ↵ ends the sentence, ⌫ corrects it. */
const BURST_EDITS: ReadonlySet<string> = new Set(["Enter", "Backspace"]);

/**
 * The guard's judgement of a key (d0126): `true` when it is part of a burst and runs nothing. A typed key counts while
 * letters run here, and right after one that did (the first letter's New ticket has the focus by then); `/` (Filter the
 * board) puts the keyboard in its field on purpose, so what follows it is typed there. Any other key ends the burst.
 */
function inBurst(b: Burst, map: Keymap, e: KeyLike, ctx: KeyContext): boolean {
  // A held key's auto-repeat isn't typing (held `j` moves on through the tickets): it neither starts nor extends a burst,
  // and one already on stays as it is.
  if (e.repeat === true) return false;
  const now = b.now();
  const fast = b.last !== null && now - b.last < BURST_MS;
  if (typesCharacter(e)) {
    if (fast) {
      b.last = now;
      const first = !b.on;
      b.on = true;
      b.dropped(first);
      return true;
    }
    b.on = false;
    b.last = null;
    if (!lettersRun(map, ctx)) return false;
    if (letterFor(map, e, ctx.letters)?.action.action !== "filter_board") b.last = now;
    b.note();
    return false;
  }
  if (b.on && fast && BURST_EDITS.has(e.key)) {
    b.last = now;
    b.dropped(false);
    return true;
  }
  b.on = false;
  b.last = null;
  return false;
}

/**
 * The actions that still run while a modal is open (`view_models/modal.rs`): Find in a focused terminal, and Terminals
 * only as leaving one (⌃`); opening the drawer behind a dialog doesn't.
 */
function runsOverModal(action: KeyAction, ctx: KeyContext): boolean {
  return action.action === "find_in_terminal" || (action.action === "terminals" && ctx.inTerminal);
}

/** Runs a binding's action (or leaves the terminal for Terminals pressed in one). */
function runBinding(d: Dispatch, binding: KeyBinding, ctx: KeyContext): KeyOutcome {
  const { action } = binding;
  const held = ctx.modal && !runsOverModal(action, ctx);
  // Terminals in a terminal leaves it; elsewhere its screen opens or closes the drawer.
  const leaves = !held && action.action === "terminals" && ctx.inTerminal;
  if (leaves) d.leaveTerminal();
  const ran = leaves || (!held && d.run(action));
  if (ran) return { kind: "ran", action };
  // A bound key does nothing else outside a terminal; in one it falls through to the terminal's own keys.
  return { kind: "unhandled", action, swallowed: !ctx.inTerminal };
}

/** Whether single letters and sequences run here (KEY-02): letters on, nobody typing, no terminal, no dialog. */
function lettersRun(map: Keymap, ctx: KeyContext): boolean {
  return map.letters && !ctx.inTerminal && !ctx.modal && ctx.letters !== "all";
}

/** Decides one key and does it (runs a handler, leaves the terminal, waits after `g`); the caller stops the event for
 * `ran`, `waiting`, `cancelled` and swallowed keys. */
export function dispatchKey(d: Dispatch, e: KeyLike, ctx: KeyContext): KeyOutcome {
  const map = d.keymap();
  if (ctx.recording) {
    d.wait(false);
    return { kind: "passed" };
  }
  if (d.waiting()) {
    d.wait(false);
    // Somebody typing now (the focus moved meanwhile) gets the key as usual.
    if (map && lettersRun(map, ctx)) {
      // A sequence's second key comes as fast as it likes (`g b`); typing on after it is a burst.
      if (d.burst) {
        d.burst.note();
        d.burst.last = d.burst.now();
      }
      let outcome: KeyOutcome = { kind: "cancelled" };
      for (const second of sequencesFor(map, e)) {
        outcome = runBinding(d, second, ctx);
        if (outcome.kind === "ran") break;
      }
      return outcome;
    }
  }
  // The off switch leaves every letter to the page, so it needs no guard (0097); nor does a terminal's typing.
  if (d.burst && map?.letters && !ctx.inTerminal && inBurst(d.burst, map, e, ctx)) return { kind: "burst" };
  if (ctx.inTerminal && (map === undefined || goesToTerminal(map, e))) return { kind: "terminal" };
  const mac = map ? map.platform === "mac" : d.mac;
  let binding = map && bindingFor(map, e, ctx.inTerminal);
  const stroke = binding && strokeOf(binding);
  if (stroke && keeps(ctx.focusKeeps, stroke)) binding = undefined;
  // ↵ / Esc (Open / close the ticket) are in 0097's single-letter table: the switch turns them off too. The key then
  // goes on to the page, so a focused control's own ↵ and Esc still work.
  if (stroke && map && !map.letters && withoutModifiers(stroke)) binding = undefined;
  if (binding) return runBinding(d, binding, ctx);
  if (map && lettersRun(map, ctx)) {
    if (startsSequence(map, e)) {
      d.wait(true);
      return { kind: "waiting" };
    }
    const letter = letterFor(map, e, ctx.letters);
    if (letter) return runBinding(d, letter, ctx);
  }
  // In a terminal the key is the terminal's (its copy and paste on Windows and Linux are Ctrl+Shift+C / V).
  // ⌥⌘C reaches the editor's text, whose Code block takes it and stops it (`ToolbarShortcuts`); it's swallowed elsewhere.
  if (!ctx.inTerminal && isWebviewKey(e, mac) && !(ctx.editorText && isCodeBlockKey(e, mac))) {
    return { kind: "swallowed" };
  }
  return { kind: "passed" };
}

/** Whether the page stops the event (the webview and the focused element don't see it). */
export function stopsEvent(outcome: KeyOutcome): boolean {
  switch (outcome.kind) {
    case "ran":
    case "waiting":
    case "cancelled":
    case "burst":
    case "swallowed":
      return true;
    case "unhandled":
      return outcome.swallowed;
    case "terminal":
    case "passed":
      return false;
  }
}

export function isModifierKey(e: KeyLike): boolean {
  return MODIFIER_KEYS.has(e.key);
}

/** The log line for a key (`OTAQ_KEY_LOG=1`). */
export function logLine(e: KeyLike, outcome: KeyOutcome): string {
  const name = keyName(e);
  const action = (a: KeyAction) => ("number" in a ? `${a.action} ${String(a.number)}` : a.action);
  switch (outcome.kind) {
    case "terminal":
      return `${name} → terminal`;
    case "ran":
      return `${name} → ${action(outcome.action)}`;
    case "unhandled":
      return `${name} → ${action(outcome.action)} (no handler${outcome.swallowed ? ", swallowed" : ""})`;
    case "waiting":
      return `${name} → waiting for the second key`;
    case "cancelled":
      return `${name} → cancelled the g wait`;
    case "burst":
      return `${name} → dropped (typed fast with nothing focused)`;
    case "swallowed":
      return `${name} → swallowed (webview key)`;
    case "passed":
      return `${name} → page`;
  }
}
