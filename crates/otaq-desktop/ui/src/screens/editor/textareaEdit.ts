// Puts a Markdown edit (`markdownEdits.ts`) into the comment box's textarea through the browser's own editing, so it
// joins the textarea's undo history: ⌘Z / Ctrl+Z undoes a toolbar click or a shortcut as it undoes typing (tuq 111).
// `execCommand("insertText")` is the one way WebKit (macOS) and WebView2 (Windows) both record an edit made by script
// in a text field's undo; setting `value` or `setRangeText` would wipe or skip it. It fires the usual `input` event, so
// React's `onChange` sees the new text. Where the command isn't there (the test runner's DOM), the text is set and the
// `input` event sent by hand: the edit then has no undo, which only a browser keeps anyway.
import type { TextEdit } from "./markdownEdits";

/** The browser's editing command, where it has one. */
function command(name: string, value?: string): boolean {
  const run = (document as { execCommand?: (name: string, ui: boolean, value?: string) => boolean })
    .execCommand;
  if (typeof run !== "function") return false;
  try {
    return run.call(document, name, false, value);
  } catch {
    return false;
  }
}

/** Replaces the edit's range with its text, as typing would, and selects what the edit says. */
export function applyEdit(el: HTMLTextAreaElement, edit: TextEdit): void {
  el.focus();
  el.setSelectionRange(edit.from, edit.to);
  const unchanged = edit.insert === el.value.slice(edit.from, edit.to);
  const done = unchanged || (edit.insert === "" ? command("delete") : command("insertText", edit.insert));
  if (!done) {
    const text = el.value.slice(0, edit.from) + edit.insert + el.value.slice(edit.to);
    // The prototype's setter: React's own on the element would take the new text as already seen, and skip onChange.
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set?.call(el, text);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }
  el.setSelectionRange(edit.selStart, edit.selEnd);
}
