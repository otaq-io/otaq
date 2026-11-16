// A comment's Markdown as static HTML (tuq 110): parsed with the description's own schema and Markdown setup
// (`markdownExtensions()`, `wrapLooseInline`), so comments and descriptions draw the same nodes, then written to the DOM
// once, with no editor. Only comments differ from descriptions, and only here:
// - a single line break is a hard break (GitHub's `breaks`), except inside code;
// - anything that looks like a tag is text: HTML is never read as markup;
// - only `https:` and `http:` links stay links; any other scheme shows as its text;
// - an image loads nothing and shows its alt text, or its URL when it has none;
// - a task list's boxes can't be ticked;
// - code blocks are highlighted as in descriptions (`highlightCode`) and, like tables, scroll sideways.
//
// The comment rules live in a Markdown parser of their own (`commentMarked`, a separate `marked` instance): the
// description's parser, `marked`'s shared default, is never touched, and its round-trip tests are the guard.

import { type Extensions, getSchema, type JSONContent } from "@tiptap/core";
import { MarkdownManager } from "@tiptap/markdown";
import { DOMSerializer, Node as PmNode, type Schema } from "@tiptap/pm/model";
import { Marked, type Tokens, Tokenizer } from "marked";

import { wrapLooseInline } from "../changes";
import { highlightCode } from "../code/highlight";
import { markdownExtensions } from "../extensions";
import { LINK_SCHEMES } from "../link/url";

/** An image as plain text: its alt text, or its URL when it has none. Nothing is loaded. */
function imageAsText(
  token: Tokens.Link | Tokens.Image | Tokens.Text | undefined,
): Tokens.Text | Tokens.Link | undefined {
  if (token?.type !== "image") return token;
  const text = token.text.trim() === "" ? token.href : token.text;
  return { type: "text", raw: token.raw, text, escaped: true };
}

/**
 * The comment parser: GitHub's single-line breaks, and no HTML. Its block `html` and inline `tag` tokenizers find
 * nothing, so `<div>`, `<script>` or `Observable<number | null>` are read as the paragraph text they are, and
 * `@tiptap/markdown` never sees an HTML token to hand to the schema. Images come out as text.
 */
function commentMarked(): Marked {
  const marked = new Marked();
  marked.use({
    gfm: true,
    breaks: true,
    tokenizer: {
      html: () => undefined,
      tag: () => undefined,
      // marked's types expect a link or an image back from `link`; its lexer takes any inline token.
      link(src) {
        return imageAsText(Tokenizer.prototype.link.call(this, src)) as Tokens.Link | undefined;
      },
      reflink(src, links) {
        return imageAsText(Tokenizer.prototype.reflink.call(this, src, links));
      },
    },
  });
  return marked;
}

interface Renderer {
  manager: MarkdownManager;
  schema: Schema;
  serializer: DOMSerializer;
}

let renderer: Renderer | null = null;

/** The parser, schema and serializer, built once for every comment. */
function commentRenderer(): Renderer {
  if (renderer === null) {
    const extensions: Extensions = markdownExtensions();
    const schema = getSchema(extensions);
    renderer = {
      manager: new MarkdownManager({ marked: commentMarked() as never, extensions }),
      schema,
      serializer: DOMSerializer.fromSchema(schema),
    };
  }
  return renderer;
}

/** Whether a link's address keeps it a link: `https:` and `http:` only (`LINK_SCHEMES`). */
export function isCommentLink(href: unknown): boolean {
  if (typeof href !== "string") return false;
  try {
    return LINK_SCHEMES.includes(new URL(href.trim()).protocol);
  } catch {
    return false;
  }
}

/** The document with every link that isn't a web link turned back into its text. */
function webLinksOnly(node: JSONContent): JSONContent {
  const marks = node.marks?.filter((mark) => mark.type !== "link" || isCommentLink(mark.attrs?.href));
  return {
    ...node,
    ...(node.marks === undefined ? {} : { marks }),
    ...(node.content === undefined ? {} : { content: node.content.map(webLinksOnly) }),
  };
}

/** The comment's Markdown as a ProseMirror document of the description's schema, or `null` if it can't be read. */
export function parseComment(markdown: string): PmNode | null {
  const { manager, schema } = commentRenderer();
  try {
    const json = webLinksOnly(wrapLooseInline(manager.parse(markdown), schema));
    if ((json.content?.length ?? 0) === 0) return null;
    const doc = schema.nodeFromJSON(json);
    doc.check();
    return doc;
  } catch (e) {
    console.error("couldn't read a comment's Markdown", e);
    return null;
  }
}

/** Colours a code block's code as a description's (`highlightCode`), from its `language-…` class. */
function highlight(code: HTMLElement): void {
  const language = [...code.classList].find((c) => c.startsWith("language-"))?.slice("language-".length);
  if (language === undefined) return;
  const doc = code.ownerDocument;
  const spans = highlightCode(language, code.textContent);
  code.replaceChildren(
    ...spans.map(({ text, token }) => {
      if (token === "plain") return doc.createTextNode(text);
      const span = doc.createElement("span");
      span.className = `tok-${token}`;
      span.textContent = text;
      return span;
    }),
  );
}

/** Wraps `el` in a new `<div class=…>`, which takes its place. */
function wrap(el: Element, className: string): void {
  const box = el.ownerDocument.createElement("div");
  box.className = className;
  el.replaceWith(box);
  box.append(el);
}

/**
 * The comment's Markdown as DOM nodes for `doc`, the way a description's read mode draws them: code blocks in the
 * description's `.md-code` frame and coloured, tables in its `.tableWrapper`, task boxes disabled, and the plain text
 * of the comment when it can't be read as Markdown. An empty comment draws nothing.
 */
export function renderCommentMarkdown(markdown: string, doc: Document): DocumentFragment {
  const fragment = doc.createDocumentFragment();
  if (markdown.trim() === "") return fragment;
  const parsed = parseComment(markdown);
  if (parsed === null) {
    const p = doc.createElement("p");
    p.className = "md-comment-plain";
    p.textContent = markdown;
    fragment.append(p);
    return fragment;
  }
  fragment.append(commentRenderer().serializer.serializeFragment(parsed.content, { document: doc }));
  for (const code of fragment.querySelectorAll<HTMLElement>("pre > code")) highlight(code);
  for (const pre of fragment.querySelectorAll("pre")) wrap(pre, "md-code");
  for (const table of fragment.querySelectorAll("table")) wrap(table, "tableWrapper");
  for (const box of fragment.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) {
    box.disabled = true;
    box.tabIndex = -1;
  }
  for (const a of fragment.querySelectorAll("a")) {
    a.removeAttribute("target");
    a.setAttribute("rel", "noopener noreferrer nofollow");
  }
  return fragment;
}

/**
 * A line that would start a block (`# …`, `> …`, `1. …`, `---`) with that mark escaped, so it reads as the paragraph
 * it is; `null` when nothing at its start could.
 */
function escapeBlockStart(line: string): string | null {
  const number = /^\d{1,9}(?=[.)])/.exec(line)?.[0];
  if (number !== undefined) return `${number}\\${line.slice(number.length)}`;
  return /^[#>+\-*_=`~|]/.test(line) ? `\\${line}` : null;
}

/** The inline nodes of `markdown` drawn as one paragraph, or `null` when it draws anything else. */
function inlineNodes(markdown: string, doc: Document): DocumentFragment | null {
  const drawn = renderCommentMarkdown(markdown, doc);
  const only = drawn.childNodes.length === 1 ? drawn.firstChild : null;
  if (!(only instanceof Element) || only.nodeName !== "P" || only.classList.contains("md-comment-plain"))
    return null;
  const fragment = doc.createDocumentFragment();
  fragment.append(...only.childNodes);
  return fragment;
}

/**
 * One line drawn as inline Markdown (tuq 110), for a text that keeps its own structure around its lines (a validation
 * report): bold, italic, inline code, struck text and web links, under the comment's rules (HTML is text, web links
 * only). A line that would read as a block (a heading, a numbered item, a quote) keeps that mark as text; one that still
 * can't be read as a single paragraph comes back as its plain text.
 */
export function renderInlineMarkdown(line: string, doc: Document): DocumentFragment {
  const text = line.replace(/\s*\n\s*/g, " ").trim();
  const escaped = escapeBlockStart(text);
  const drawn = inlineNodes(text, doc) ?? (escaped === null ? null : inlineNodes(escaped, doc));
  if (drawn !== null) return drawn;
  const fragment = doc.createDocumentFragment();
  if (text !== "") fragment.append(doc.createTextNode(text));
  return fragment;
}
