// @vitest-environment happy-dom
// A comment drawn as Markdown (tuq 110, plan comments-as-markdown piece 1): the description's nodes with no editor, and
// the rules only comments have: single line breaks kept, HTML never read as markup, web links only, images not loaded,
// task boxes that can't be ticked, small headings, coloured code, tables and code that scroll. Then sample texts shaped
// like real comments, the empty comment, and the description's own parsing left as it was.

import { Editor } from "@tiptap/core";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, test, vi } from "vitest";

import { markdownExtensions } from "../extensions";
import { CommentMarkdown } from "./CommentMarkdown";
import { isCommentLink, renderCommentMarkdown, renderInlineMarkdown } from "./render";

const requests = vi.hoisted(() => [] as [string, unknown][]);
vi.mock(import("../../tauri"), async (importOriginal) => ({
  ...(await importOriginal()),
  request: (method: string, params: unknown) => {
    requests.push([method, params]);
    return Promise.resolve(null);
  },
}));

/** The comment drawn into a detached element. */
function draw(markdown: string): HTMLElement {
  const el = document.createElement("div");
  el.append(renderCommentMarkdown(markdown, document));
  return el;
}

beforeAll(async () => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  // The test runner leaves CSS imports empty: the description's, its code's and the comment's style sheets, read as
  // the app loads them.
  const fsModule = "node:fs";
  const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
  const here = (import.meta as ImportMeta & { dirname: string }).dirname;
  const style = document.createElement("style");
  style.textContent = ["../markdown.css", "../code/code.css", "comment.css"]
    .map((sheet) => fs.readFileSync(`${here}/${sheet}`, "utf8"))
    .join("\n");
  document.head.append(style);
});

beforeEach(() => {
  requests.length = 0;
});

describe("line breaks", () => {
  test("a single line break is a hard break; a blank line starts a new paragraph", () => {
    const el = draw("line one\nline two\n\nnext paragraph");
    const paragraphs = el.querySelectorAll(":scope > p");
    expect(paragraphs).toHaveLength(2);
    expect(paragraphs[0]?.innerHTML).toBe("line one<br>line two");
    expect(paragraphs[1]?.textContent).toBe("next paragraph");
  });

  test("a line break inside a list item or a quote stays a break", () => {
    const el = draw("- one\n  more\n\n> quoted\n> again");
    expect(el.querySelector("li > p")?.innerHTML).toBe("one<br>more");
    expect(el.querySelector("blockquote > p")?.innerHTML).toBe("quoted<br>again");
  });

  test("a task's second line stays a line of its own (the description's task item reads it as a paragraph)", () => {
    const el = draw("- [ ] task\n  detail");
    expect([...el.querySelectorAll("li > div > p")].map((p) => p.textContent)).toEqual(["task", "detail"]);
  });

  test("inside a code block a line break is the code's own, not a hard break", () => {
    const el = draw("```\nfirst\nsecond\n```");
    expect(el.querySelector("pre")?.textContent).toBe("first\nsecond");
    expect(el.querySelector("br")).toBeNull();
  });
});

describe("HTML is never read as markup", () => {
  test.each([
    ["generics", "the hook returns Observable<number | null> now"],
    ["a placeholder", "explain <why> in one line"],
    ["placeholders in a file name", "screenshots are named NN-<page>-<step>.png"],
    ["a pasted div", '<div class="x">\nhello\n</div>'],
    ["a script", "<script>alert(1)</script>"],
    ["an image with onerror", '<img src="x" onerror="alert(1)">'],
    ["inline tags", 'some <b>bold</b> and <i>italic</i> <a href="https://e.com">link</a>'],
    ["an HTML block", "<details>\n<summary>More</summary>\n\nhidden text\n\n</details>"],
    ["an iframe", '<iframe src="https://e.com"></iframe>'],
    ["a comment", "before <!-- hidden --> after"],
    ["a style", "<style>body { display: none }</style>"],
  ])("%s shows as the text written", (_name, markdown) => {
    const el = draw(markdown);
    for (const tag of ["div", "script", "img", "b", "i", "a", "details", "summary", "iframe", "style"])
      expect(el.querySelector(tag), tag).toBeNull();
    // Every character written is still there, the line breaks aside.
    const shown = el.textContent.replace(/\s+/g, "");
    expect(shown).toBe(markdown.replace(/\s+/g, ""));
  });

  test("no attribute written in a comment reaches the page", () => {
    const el = draw('<img src="x" onerror="alert(1)"> <p onclick="alert(2)">x</p> <svg onload="alert(3)">');
    expect(el.querySelector("[onerror], [onclick], [onload], svg, img")).toBeNull();
  });

  test("HTML inside code stays code", () => {
    const el = draw("`<div>` and\n\n```html\n<div>x</div>\n```");
    expect(el.querySelector("p > code")?.textContent).toBe("<div>");
    expect(el.querySelector("pre")?.textContent).toBe("<div>x</div>");
  });
});

describe("links", () => {
  test.each([
    ["https", "[the report](https://example.com/report)", "https://example.com/report"],
    ["http", "[local](http://localhost:4000/a)", "http://localhost:4000/a"],
    ["an autolink", "<https://example.com>", "https://example.com"],
    ["a bare URL", "see https://example.com/x for more", "https://example.com/x"],
  ])("%s is a link", (_name, markdown, href) => {
    expect(draw(markdown).querySelector("a")?.getAttribute("href")).toBe(href);
  });

  test.each([
    ["javascript:", "[click me](javascript:alert(1))", "click me"],
    ["javascript: autolink", "<javascript:alert(1)>", "javascript:alert(1)"],
    ["JavaScript: in capitals", "[x](JavaScript:alert(1))", "x"],
    ["mailto:", "[mail](mailto:team@example.com)", "mail"],
    ["an email autolink", "<team@example.com>", "team@example.com"],
    ["file:", "[secret](file:///etc/passwd)", "secret"],
    ["data:", "[d](data:text/html,<script>alert(1)</script>)", "d"],
    ["otaq:", "[t](otaq://ticket/SHOP-47)", "t"],
    ["a bare ftp URL", "ftp://example.com/file", "ftp://example.com/file"],
    ["a relative path", "[notes](./notes.md)", "notes"],
  ])("%s shows as text", (_name, markdown, text) => {
    const el = draw(markdown);
    expect(el.querySelector("a")).toBeNull();
    expect(el.textContent).toContain(text);
  });

  test("the scheme check is LINK_SCHEMES'", () => {
    expect(isCommentLink("https://e.com")).toBe(true);
    expect(isCommentLink(" HTTP://e.com ")).toBe(true);
    expect(isCommentLink("javascript:alert(1)")).toBe(false);
    expect(isCommentLink("not a url")).toBe(false);
    expect(isCommentLink(undefined)).toBe(false);
  });
});

describe("images load nothing", () => {
  test("an image shows its alt text", () => {
    const el = draw("look: ![the login page](https://example.com/shot.png)");
    expect(el.querySelector("img")).toBeNull();
    expect(el.textContent).toBe("look: the login page");
  });

  test("an image with no alt text shows its URL", () => {
    const el = draw("![](https://example.com/shot.png)");
    expect(el.querySelector("img")).toBeNull();
    expect(el.textContent).toBe("https://example.com/shot.png");
  });

  test("a reference image and a linked image load nothing either", () => {
    const el = draw(
      "![ref shot][s] and [![badge](https://e.com/b.svg)](https://e.com)\n\n[s]: https://e.com/s.png",
    );
    expect(el.querySelector("img")).toBeNull();
    expect(el.textContent).toBe("ref shot and badge");
  });
});

describe("the description's nodes", () => {
  test("task lists show their boxes, which can't be ticked", () => {
    const el = draw("- [ ] to do\n- [x] done");
    const boxes = [
      ...el.querySelectorAll<HTMLInputElement>('ul[data-type="taskList"] input[type="checkbox"]'),
    ];
    expect(boxes.map((b) => [b.checked, b.disabled])).toEqual([
      [false, true],
      [true, true],
    ]);
    boxes[0]?.click();
    expect(boxes[0]?.checked).toBe(false);
  });

  test("headings, quotes, lists, rules, bold, italic and inline code are the editor's elements", () => {
    const el = draw(
      "# Title\n\n> a quote\n\n1. one\n2. two\n\n- a\n- b\n\n---\n\n**bold** *it* `code` ~~gone~~",
    );
    expect([...el.children].map((c) => c.tagName.toLowerCase())).toEqual([
      "h1",
      "blockquote",
      "ol",
      "ul",
      "hr",
      "p",
    ]);
    expect(el.querySelector(":scope > p")?.innerHTML).toBe(
      "<strong>bold</strong> <em>it</em> <code>code</code> <s>gone</s>",
    );
  });

  test("a code block is coloured as a description's and sits in its scrolling frame", () => {
    const el = draw("```ts\nconst a = 1;\n```");
    const frame = el.querySelector(".md-code");
    expect(frame?.firstElementChild?.tagName).toBe("PRE");
    expect(frame?.querySelector(".tok-keyword")?.textContent).toBe("const");
    expect(frame?.querySelector(".tok-number")?.textContent).toBe("1");
  });

  test("a code block in a language the picker doesn't offer stays plain", () => {
    const el = draw("```cobol\nDISPLAY 'HI'.\n```");
    expect(el.querySelector("pre")?.textContent).toBe("DISPLAY 'HI'.");
    expect(el.querySelector("[class^='tok-']")).toBeNull();
  });

  test("a table sits in the description's sideways-scrolling wrapper", () => {
    const el = draw("| a | b |\n|---|---|\n| 1 | 2 |");
    const table = el.querySelector(".tableWrapper > table");
    expect(table?.querySelectorAll("th")).toHaveLength(2);
    expect(table?.querySelectorAll("td")).toHaveLength(2);
  });
});

describe("the comment's styles", () => {
  /** `markdown` drawn in a frame with these classes, in the page so the style sheets apply. */
  function framed(markdown: string, classes: string): HTMLElement {
    const frame = document.body.appendChild(document.createElement("div"));
    frame.className = classes;
    const text = frame.appendChild(document.createElement("div"));
    text.className = "ProseMirror";
    text.setAttribute("contenteditable", "false");
    text.append(renderCommentMarkdown(markdown, document));
    return frame;
  }

  afterEach(() => {
    document.body.replaceChildren();
  });

  const HEADINGS = "# One\n\n## Two\n\n### Three\n\n###### Six";
  const size = (el: Element | null | undefined) => (el ? getComputedStyle(el).fontSize : null);

  // Bold text is the text's own size, 13 px (markdown.css's `font: 400 13px/1.45`, a shorthand happy-dom doesn't
  // resolve), drawn 600 like a heading.
  test("every heading in a comment is the size of bold text", () => {
    const frame = framed(HEADINGS, "md-editor md-comment");
    const headings = [...frame.querySelectorAll("h1, h2, h3, h6")];
    expect(headings).toHaveLength(4);
    for (const h of headings) expect(size(h), h.tagName).toBe("13px");
  });

  test("a description keeps its heading sizes (markdown.css unchanged)", () => {
    const frame = framed(HEADINGS, "md-editor");
    expect(size(frame.querySelector("h1"))).toBe("17px");
    expect(size(frame.querySelector("h2"))).toBe("15px");
  });

  test("code blocks and tables scroll sideways", () => {
    const frame = framed("```\nlong line\n```\n\n| a |\n|---|\n| 1 |", "md-editor md-comment");
    const pre = frame.querySelector(".md-code > pre");
    expect(pre && getComputedStyle(pre).overflowX).toBe("auto");
    expect(pre && getComputedStyle(pre).whiteSpace).toBe("pre");
    const wrapper = frame.querySelector(".tableWrapper");
    expect(wrapper && getComputedStyle(wrapper).overflowX).toBe("auto");
  });
});

describe("comments shaped like real ones (110)", () => {
  test("a hand-over with bold, backticks, placeholders and glob stars", () => {
    const markdown = [
      "**Done.** Moved `parseComment` into `markdown/comment/render.ts`.",
      "Files: `before-*`, `after-*` and NN-<page>-<step>.png are left alone.",
      "Next: run `npx vitest run <file>` then explain <why>.",
    ].join("\n");
    const el = draw(markdown);
    const p = el.querySelector("p");
    expect(el.children).toHaveLength(1);
    expect(p?.querySelectorAll("br")).toHaveLength(2);
    expect(p?.querySelector("strong")?.textContent).toBe("Done.");
    expect([...(p?.querySelectorAll("code") ?? [])].map((c) => c.textContent)).toEqual([
      "parseComment",
      "markdown/comment/render.ts",
      "before-*",
      "after-*",
      "npx vitest run <file>",
    ]);
    expect(p?.textContent).toContain("NN-<page>-<step>.png");
    expect(p?.textContent).toContain("explain <why>.");
  });

  test("glob stars outside code may set a few words in italics, and every word stays", () => {
    const el = draw("compare before-*, after-* screenshots");
    expect(el.textContent).toBe("compare before-, after- screenshots");
  });

  test("a one-line-per-criterion list keeps its lines", () => {
    const markdown = [
      "Checked against the criteria:",
      "✓ the thread shows Markdown",
      "✓ a single line break stays",
      "✗ headings: still too big",
    ].join("\n");
    const p = draw(markdown).querySelector("p");
    expect(p?.innerHTML.split("<br>")).toEqual(markdown.split("\n"));
  });

  test("a Markdown hand-over with a heading, a list and code", () => {
    const el = draw("## Summary\n- added `CommentMarkdown`\n- tests pass\n\n```bash\nnpx vitest run\n```");
    expect(el.querySelector("h2")?.textContent).toBe("Summary");
    expect(el.querySelectorAll("li")).toHaveLength(2);
    expect(el.querySelector(".md-code pre")?.textContent).toBe("npx vitest run");
  });

  test("an entity is read as its character, as GitHub does", () => {
    expect(draw("a &lt;b&gt; &amp; c").textContent).toBe("a <b> & c");
  });
});

describe("an empty comment", () => {
  test.each([[""], ["   "], ["\n\n"]])("%j draws nothing", (markdown) => {
    expect(draw(markdown).childNodes).toHaveLength(0);
  });
});

describe("<CommentMarkdown>", () => {
  let root: Root | null = null;
  let host: HTMLElement | null = null;

  function render(markdown: string): HTMLElement {
    host ??= document.body.appendChild(document.createElement("div"));
    root ??= createRoot(host);
    act(() => root?.render(createElement(CommentMarkdown, { markdown, className: "comment-body" })));
    return host;
  }

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
  });

  test("draws the comment in the description's read-mode frame, with no editor", () => {
    const page = render("**hi**\nthere");
    const frame = page.querySelector(".md-editor.md-comment.comment-body");
    const text = frame?.querySelector(".ProseMirror");
    expect(text?.getAttribute("contenteditable")).toBe("false");
    expect(text?.innerHTML).toBe("<p><strong>hi</strong><br>there</p>");
    expect(page.querySelector('[contenteditable="true"]')).toBeNull();
  });

  test("draws a new text when the comment changes", () => {
    render("first");
    const page = render("second");
    expect(page.querySelector(".ProseMirror")?.textContent).toBe("second");
  });

  test("a click on a link opens it in the system browser, never in the page", () => {
    const page = render("see [the report](https://example.com/report)");
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    page.querySelector("a")?.dispatchEvent(click);
    expect(click.defaultPrevented).toBe(true);
    expect(requests).toEqual([["terminal.open_link", { url: "https://example.com/report" }]]);
  });

  test("a middle click on a link opens nothing", () => {
    const page = render("[r](https://example.com/report)");
    const click = new MouseEvent("auxclick", { bubbles: true, cancelable: true, button: 1 });
    page.querySelector("a")?.dispatchEvent(click);
    expect(click.defaultPrevented).toBe(true);
    expect(requests).toEqual([]);
  });

  test("a click on the text outside a link does nothing", () => {
    const page = render("plain [javascript](javascript:alert(1))");
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    page.querySelector("p")?.dispatchEvent(click);
    expect(click.defaultPrevented).toBe(false);
    expect(requests).toEqual([]);
  });

  test("50 comments start no editor", () => {
    const comments = Array.from({ length: 50 }, (_, i) => `comment **${String(i)}**`);
    host = document.body.appendChild(document.createElement("div"));
    root = createRoot(host);
    act(() =>
      root?.render(
        createElement(
          "div",
          null,
          comments.map((markdown, i) => createElement(CommentMarkdown, { key: i, markdown })),
        ),
      ),
    );
    expect(host.querySelectorAll(".md-comment")).toHaveLength(50);
    expect(host.querySelectorAll('[contenteditable="true"], .tiptap')).toHaveLength(0);
  });
});

describe("the description's parsing is left as it was", () => {
  /** A description's document, as the editor reads it. */
  function description(markdown: string): string {
    const editor = new Editor({
      element: null,
      extensions: markdownExtensions(),
      content: markdown,
      contentType: "markdown",
    });
    try {
      return JSON.stringify(editor.getJSON());
    } finally {
      editor.destroy();
    }
  }

  test("drawing comments changes nothing in how a description is read", () => {
    const markdown = "a\nb <b>x</b>\n\n<div>block</div>\n\n![logo](https://e.com/l.png)";
    const before = description(markdown);
    draw(markdown);
    expect(description(markdown)).toBe(before);
    expect(before).not.toContain("hardBreak");
    expect(before).toContain('"type":"bold"');
  });
});

describe("one line as inline Markdown (a validation report's line)", () => {
  /** The line drawn into a detached element. */
  const inline = (line: string) => {
    const el = document.createElement("span");
    el.append(renderInlineMarkdown(line, document));
    return el;
  };

  test("draws bold, italic, inline code and web links, with no paragraph around them", () => {
    const el = inline("**Done**: `node test.js` passed, *see* [the run](https://e.com/run)");
    expect(el.querySelector("p")).toBeNull();
    expect(el.querySelector("strong")?.textContent).toBe("Done");
    expect(el.querySelector("code")?.textContent).toBe("node test.js");
    expect(el.querySelector("em")?.textContent).toBe("see");
    expect(el.querySelector("a")?.getAttribute("href")).toBe("https://e.com/run");
    expect(el.textContent).toBe("Done: node test.js passed, see the run");
  });

  test.each([
    ["a heading's mark", "# not a heading"],
    ["a numbered item's mark", "1. not a list"],
    ["a bullet's mark", "- not a list"],
    ["a quote's mark", "> not a quote"],
    ["a rule", "---"],
  ])("%s stays text: a line is never a block", (_, line) => {
    const el = inline(line);
    expect(el.textContent).toBe(line);
    expect(el.querySelector("h1, ol, ul, li, blockquote, hr, p")).toBeNull();
  });

  test("HTML stays text, and a link that isn't a web link is its text", () => {
    const el = inline("returns Observable<number | null>, see [x](javascript:alert(1))");
    expect(el.textContent).toBe("returns Observable<number | null>, see x");
    expect(el.querySelector("a")).toBeNull();
  });

  test("an empty line draws nothing", () => {
    expect(inline("  ").childNodes).toHaveLength(0);
  });
});
