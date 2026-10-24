// No smart punctuation or autocorrect in what the user types (#41, #52): titles, branch names, commands and code
// stay as typed (`'--port'`, never `‘—port’`). Every text `<input>` and `<textarea>` spreads one of these, and
// `plainText.test.ts` fails on a field that doesn't. The app also switches macOS's text substitutions off for its
// own defaults domain before the webview opens (`src/door.rs`, `text_substitutions_off`).

/** Code-like text (titles, names, branches, commands, search): no autocorrect, no capitals, no spell check. */
export const PLAIN_TEXT = { autoCorrect: "off", autoCapitalize: "off", spellCheck: false } as const;

/** Prose (a comment, a note): no autocorrect or capitals either, but misspellings keep their underline. */
export const PROSE_TEXT = { autoCorrect: "off", autoCapitalize: "off" } as const;

/** [`PLAIN_TEXT`] on an element the page doesn't render itself (xterm.js's helper textarea). */
export function plainTextOff(element: HTMLElement): void {
  element.setAttribute("autocorrect", "off");
  element.setAttribute("autocapitalize", "off");
  element.setAttribute("spellcheck", "false");
}
