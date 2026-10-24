// @vitest-environment happy-dom
// Copying from programs (d0117 point 7, T-0510), through the real xterm.js parser and clipboard add-on: an OSC 52
// write copies, a query gets an empty reply and never reads the clipboard, a clear changes nothing, a replay never
// copies.
import { Terminal } from "@xterm/xterm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { copyFromPrograms } from "./clipboard";
import { encodeBase64 } from "./input";

const b64 = (text: string) => encodeBase64(new TextEncoder().encode(text));

describe("copying from programs", () => {
  let term: Terminal;
  let copied: string[];
  let sent: string[];
  let replaying: boolean;
  // A stand-in for the system clipboard: nothing in a test may touch the real one.
  const clipboard = { readText: vi.fn(), writeText: vi.fn() };

  beforeEach(() => {
    Object.defineProperty(navigator, "clipboard", { value: clipboard, configurable: true });
    clipboard.readText.mockClear();
    clipboard.writeText.mockClear();
    term = new Terminal({ allowProposedApi: true });
    copied = [];
    sent = [];
    replaying = false;
    term.loadAddon(
      copyFromPrograms(
        () => replaying,
        (text) => copied.push(text),
      ),
    );
    term.onData((data) => sent.push(data));
  });

  afterEach(() => {
    term.dispose();
    expect(clipboard.readText).not.toHaveBeenCalled();
    expect(clipboard.writeText).not.toHaveBeenCalled();
  });

  const write = (text: string) =>
    new Promise<void>((resolve) => {
      term.write(text, resolve);
    });

  it("puts a program's copy on the clipboard (BEL and ST endings, any selection, UTF-8)", async () => {
    await write(`\x1b]52;c;${b64("npm run build")}\x07`);
    await write(`\x1b]52;c;${b64("second")}\x1b\\`);
    await write(`\x1b]52;;${b64("from tmux")}\x07`);
    await write(`\x1b]52;p;${b64("ünïcødé ✻")}\x07`);
    expect(copied).toEqual(["npm run build", "second", "from tmux", "ünïcødé ✻"]);
    expect(sent).toEqual([]);
  });

  it("answers a query with an empty clipboard, never reading it", async () => {
    await write("\x1b]52;c;?\x07");
    await write("\x1b]52;p;?\x1b\\");
    expect(copied).toEqual([]);
    expect(sent).toEqual(["\x1b]52;c;\x07", "\x1b]52;p;\x07"]);
  });

  it("leaves the clipboard as it was on a clear or a payload that isn't base64", async () => {
    await write(`\x1b]52;c;${b64("kept")}\x07`);
    await write("\x1b]52;c;\x07");
    await write("\x1b]52;c;not base64!\x07");
    await write("\x1b]52;c\x07");
    expect(copied).toEqual(["kept"]);
  });

  it("never copies from replayed output", async () => {
    replaying = true;
    await write(`\x1b]52;c;${b64("old copy")}\x07`);
    replaying = false;
    await write(`\x1b]52;c;${b64("new copy")}\x07`);
    expect(copied).toEqual(["new copy"]);
  });
});
