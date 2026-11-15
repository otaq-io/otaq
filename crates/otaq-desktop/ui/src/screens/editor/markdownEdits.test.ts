// The comment box's Markdown edits (tuq 111, plan comments-as-markdown piece 7): each format wraps and unwraps, on a
// selection and at the caret. A text is written with its selection marked: `[fix]` is a selection, `|` the caret.
import { describe, expect, it } from "vitest";

import type { CommentFormat } from "../../generated/CommentFormat";
import { applied, formatEdit, type Selected } from "./markdownEdits";

/** "a [fix] b" → the text and its selection; "a | b" → the caret. */
function at(marked: string): Selected {
  const caret = marked.indexOf("|");
  if (caret >= 0) return { text: marked.replace("|", ""), start: caret, end: caret };
  const start = marked.indexOf("[");
  const end = marked.indexOf("]") - 1;
  return { text: marked.replace("[", "").replace("]", ""), start, end };
}

/** The text after `format`, its selection marked the same way. */
function after(format: CommentFormat, marked: string): string {
  const s = at(marked);
  const edit = formatEdit(format, s);
  const text = applied(s.text, edit);
  if (edit.selStart === edit.selEnd) return `${text.slice(0, edit.selStart)}|${text.slice(edit.selStart)}`;
  return `${text.slice(0, edit.selStart)}[${text.slice(edit.selStart, edit.selEnd)}]${text.slice(edit.selEnd)}`;
}

/** The text after `format` with all of it selected (for texts whose task boxes hold brackets). */
function whole(format: CommentFormat, text: string): string {
  return applied(text, formatEdit(format, { text, start: 0, end: text.length }));
}

/** Link texts hold brackets of their own, so their selections are given as offsets. */
function link(text: string, start: number, end = start) {
  const edit = formatEdit("link", { text, start, end });
  const out = applied(text, edit);
  return { text: out, selected: out.slice(edit.selStart, edit.selEnd), caret: edit.selStart };
}

describe("bold, italic and code", () => {
  it("wrap a selection and select the same words", () => {
    expect(after("bold", "a [fix] b")).toBe("a **[fix]** b");
    expect(after("italic", "a [fix] b")).toBe("a _[fix]_ b");
    expect(after("code", "a [fix] b")).toBe("a `[fix]` b");
  });

  it("insert their marks with the caret between them when nothing is selected", () => {
    expect(after("bold", "a |b")).toBe("a **|**b");
    expect(after("italic", "|")).toBe("_|_");
    expect(after("code", "x|")).toBe("x`|`");
  });

  it("unwrap a selection inside the marks", () => {
    expect(after("bold", "a **[fix]** b")).toBe("a [fix] b");
    expect(after("italic", "a _[fix]_ b")).toBe("a [fix] b");
    expect(after("code", "a `[fix]` b")).toBe("a [fix] b");
  });

  it("unwrap a selection holding the marks", () => {
    expect(after("bold", "a [**fix**] b")).toBe("a [fix] b");
    expect(after("italic", "a [_fix_] b")).toBe("a [fix] b");
    expect(after("code", "a [`fix`] b")).toBe("a [fix] b");
  });

  it("take empty marks away at the caret", () => {
    expect(after("bold", "a **|** b")).toBe("a | b");
    expect(after("italic", "_|_")).toBe("|");
    expect(after("code", "`|`")).toBe("|");
  });

  it("leave the spaces at a selection's ends outside the marks", () => {
    expect(after("bold", "a[ fix ]b")).toBe("a **[fix]** b");
  });

  it("make a fenced block of code over several lines, on lines of its own", () => {
    expect(after("code", "see [a\nb] now")).toBe("see \n```\n[a\nb]\n```\n now");
    expect(after("code", "[a\nb]")).toBe("```\n[a\nb]\n```");
    expect(after("code", "```\n[a\nb]\n```")).toBe("[a\nb]");
  });
});

describe("link", () => {
  it("wraps selected words and selects url for typing over", () => {
    expect(link("see the docs", 8, 12)).toEqual({ text: "see the [docs](url)", selected: "url", caret: 15 });
  });

  it("gives a selected URL empty brackets with the caret in them", () => {
    const url = "https://example.com/a";
    expect(link(`go ${url} now`, 3, 3 + url.length)).toEqual({
      text: `go [](${url}) now`,
      selected: "",
      caret: 4,
    });
  });

  it("inserts [](url) with the caret in the brackets when nothing is selected", () => {
    expect(link("go ", 3)).toEqual({ text: "go [](url)", selected: "", caret: 4 });
  });

  it("unwraps a selected link, or a selection that is a link's text", () => {
    expect(link("a [docs](https://x.io) b", 2, 22)).toEqual({ text: "a docs b", selected: "docs", caret: 2 });
    expect(link("a [docs](https://x.io) b", 3, 7)).toEqual({ text: "a docs b", selected: "docs", caret: 2 });
  });
});

describe("lists and quote", () => {
  it("mark the caret's line, keeping the caret on its words", () => {
    expect(after("bullet_list", "one\ntw|o\nthree")).toBe("one\n- tw|o\nthree");
    expect(after("numbered_list", "tw|o")).toBe("1. tw|o");
    expect(after("task_list", "tw|o")).toBe("- [ ] tw|o");
    expect(after("quote", "tw|o")).toBe("> tw|o");
  });

  it("mark an empty line, with the caret after the marker", () => {
    expect(after("bullet_list", "|")).toBe("- |");
    expect(after("numbered_list", "a\n|")).toBe("a\n1. |");
    expect(after("task_list", "|")).toBe("- [ ] |");
    expect(after("quote", "|")).toBe("> |");
  });

  it("mark every line the selection touches and select them", () => {
    expect(after("bullet_list", "o[ne\ntw]o\nthree")).toBe("[- one\n- two]\nthree");
    expect(after("numbered_list", "[one\ntwo\nthree]")).toBe("[1. one\n2. two\n3. three]");
    expect(after("task_list", "[one\ntwo]")).toBe("[- [ ] one\n- [ ] two]");
    expect(after("quote", "[one\n\ntwo]")).toBe("[> one\n>\n> two]");
  });

  it("leave blank lines among list lines alone, and count only the others", () => {
    expect(after("numbered_list", "[one\n\ntwo]")).toBe("[1. one\n\n2. two]");
  });

  it("don't take in the next line when the selection ends at its start", () => {
    expect(after("bullet_list", "[one\n]two")).toBe("- [one\n]two");
  });

  it("are taken away when every line has them", () => {
    expect(after("bullet_list", "[- one\n- two]")).toBe("[one\ntwo]");
    expect(after("numbered_list", "[1. one\n2. two]")).toBe("[one\ntwo]");
    expect(whole("task_list", "- [ ] one\n- [x] two")).toBe("one\ntwo");
    expect(after("quote", "[> one\n>\n> two]")).toBe("[one\n\ntwo]");
    expect(after("bullet_list", "- tw|o")).toBe("tw|o");
    expect(after("quote", "> tw|o")).toBe("tw|o");
  });

  it("replace another list's markers, keeping the indent", () => {
    expect(after("numbered_list", "[- one\n  - two]")).toBe("[1. one\n  2. two]");
    expect(after("task_list", "[1. one\n2. two]")).toBe("[- [ ] one\n- [ ] two]");
    expect(whole("bullet_list", "- [ ] one\nthree")).toBe("- one\n- three");
  });
});
