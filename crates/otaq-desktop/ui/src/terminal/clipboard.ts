// Copying from programs (d0117 point 7): a program that copies through the terminal (OSC 52: Claude Code's copy, vim,
// tmux) puts its text on the clipboard, through xterm.js's clipboard add-on with Otaq's provider. Write only: a
// program asking for the clipboard (`?`) gets the add-on's empty reply, and output replayed from an earlier moment
// never copies again.
import { ClipboardAddon, type IClipboardProvider } from "@xterm/addon-clipboard";

/**
 * Otaq's clipboard for programs: `readText` reads nothing, so a query is answered with an empty clipboard;
 * `writeText` hands the text to `write` unless `replaying()` says the output is a replay. An empty text (a clear, or a
 * payload that isn't base64) leaves the user's clipboard as it is. Every selection (`c`, `p`, `s`, none) is the one
 * clipboard.
 */
export function otaqClipboard(replaying: () => boolean, write: (text: string) => void): IClipboardProvider {
  return {
    readText: () => "",
    writeText: (_selection, text) => {
      if (text !== "" && !replaying()) write(text);
    },
  };
}

/** The add-on for one terminal (`term.loadAddon`), copying through [`otaqClipboard`]. */
export function copyFromPrograms(replaying: () => boolean, write: (text: string) => void): ClipboardAddon {
  return new ClipboardAddon(undefined, otaqClipboard(replaying, write));
}
