// @vitest-environment happy-dom
// Every text field types plain text (#41, #52): a new `<input>` or `<textarea>` that doesn't spread `PLAIN_TEXT` or
// `PROSE_TEXT` fails here, before macOS can turn `'--port'` into `‘—port’` in it.
import { describe, expect, it } from "vitest";

import { PLAIN_TEXT, plainTextOff, PROSE_TEXT } from "./plainText";

const sources = import.meta.glob<string>(["../**/*.tsx", "!../**/*.test.*"], {
  query: "?raw",
  import: "default",
  eager: true,
});

/** Each `<input …>` / `<textarea …>` opening tag in `source`, braces counted so `=>` inside props doesn't end it. */
function openingTags(source: string): string[] {
  const tags: string[] = [];
  for (const match of source.matchAll(/<(input|textarea)\b/g)) {
    let depth = 0;
    let end = match.index + match[0].length;
    for (; end < source.length; end++) {
      const c = source[end];
      if (c === "{") depth++;
      else if (c === "}") depth--;
      else if (c === ">" && depth === 0) break;
    }
    tags.push(source.slice(match.index, end + 1));
  }
  return tags;
}

describe("plain text in every field", () => {
  it("finds the page's fields", () => {
    const all = Object.values(sources).flatMap(openingTags);
    expect(all.length).toBeGreaterThan(30);
  });

  it("every text <input> and <textarea> spreads PLAIN_TEXT or PROSE_TEXT", () => {
    const missing = Object.entries(sources).flatMap(([file, source]) =>
      openingTags(source)
        .filter((tag) => !/type="(checkbox|radio)"/.test(tag))
        .filter((tag) => !/\{\.\.\.(PLAIN_TEXT|PROSE_TEXT)\}/.test(tag))
        .map((tag) => `${file}: ${tag.split("\n")[0] ?? tag}`),
    );
    expect(missing).toEqual([]);
  });

  it("turn autocorrect and capitals off; only prose keeps spell check", () => {
    expect(PLAIN_TEXT).toEqual({ autoCorrect: "off", autoCapitalize: "off", spellCheck: false });
    expect(PROSE_TEXT).toEqual({ autoCorrect: "off", autoCapitalize: "off" });
  });

  it("plainTextOff sets the same attributes on an element the page doesn't render", () => {
    const textarea = document.createElement("textarea");
    plainTextOff(textarea);
    expect(textarea.getAttribute("autocorrect")).toBe("off");
    expect(textarea.getAttribute("autocapitalize")).toBe("off");
    expect(textarea.getAttribute("spellcheck")).toBe("false");
  });
});
