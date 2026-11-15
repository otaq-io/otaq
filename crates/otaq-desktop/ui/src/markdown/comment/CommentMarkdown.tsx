// A comment drawn as Markdown (tuq 110): read-only static HTML from `renderCommentMarkdown`, in the description's read-mode
// frame (`.md-editor .ProseMirror`, not editable), so markdown.css and code.css draw a comment's nodes the way they draw a
// description's. No Tiptap editor is started: a thread of 50 comments is 50 pieces of HTML. comment.css keeps what only
// comments differ in (small headings).

import { useLayoutEffect, useRef } from "react";

import "../code/code.css";
import { openInBrowser } from "../link/url";
import "../markdown.css";
import "./comment.css";
import { renderCommentMarkdown } from "./render";

export interface CommentMarkdownProps {
  /** The comment's text as written. */
  markdown: string;
  /** Classes added to the outer element. */
  className?: string;
}

/**
 * A press on a link opens it in the system browser, as a description's read mode does, and never in the page. A link
 * that isn't a web link can't be here (render.ts drops it); `openInBrowser` checks again.
 */
function openLinks(el: HTMLElement): () => void {
  const onClick = (e: MouseEvent) => {
    const a = e.target instanceof Element ? e.target.closest("a") : null;
    if (a === null || !el.contains(a)) return;
    e.preventDefault();
    if (e.type === "click") openInBrowser(a.getAttribute("href") ?? "");
  };
  el.addEventListener("click", onClick);
  el.addEventListener("auxclick", onClick);
  return () => {
    el.removeEventListener("click", onClick);
    el.removeEventListener("auxclick", onClick);
  };
}

export function CommentMarkdown({ markdown, className }: CommentMarkdownProps) {
  const text = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = text.current;
    if (el) el.replaceChildren(renderCommentMarkdown(markdown, el.ownerDocument));
  }, [markdown]);

  useLayoutEffect(() => {
    const el = text.current;
    return el ? openLinks(el) : undefined;
  }, []);

  return (
    <div className={className === undefined ? "md-editor md-comment" : `md-editor md-comment ${className}`}>
      <div ref={text} className="ProseMirror" contentEditable={false} />
    </div>
  );
}
