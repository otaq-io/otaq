// An agent question's text, the same wherever the answer UI draws it: the first line is the question, bold; the rest is
// its context, regular weight, at a readable line length, simple Markdown drawn (`questionContext`). The question line
// and the context are one text that scrolls together when its place caps it, while the options and Send stay in view.
// The split itself is `otaq-view`'s (`SheetAnswer.line`, `.context`).
import {
  type ReactNode,
  type Ref,
  type RefObject,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import type { SheetAnswer } from "../../generated/SheetAnswer";
import { Icon } from "../../theme/Icon";
import { type Span, contextBlocks } from "./questionContext";

function Spans({ spans }: { spans: Span[] }): ReactNode {
  return spans.map((span, i) => {
    switch (span.kind) {
      case "code":
        return (
          <code key={i} className="sheet-answer-code">
            {span.text}
          </code>
        );
      case "bold":
        return <strong key={i}>{span.text}</strong>;
      case "text":
        return span.text;
    }
  });
}

/** The context's blocks: paragraphs (line breaks kept), lists, code blocks, headings as bold paragraphs, tables as
 * plain text. */
function QuestionContext({ context }: { context: string }) {
  return (
    <div className="sheet-answer-context">
      {contextBlocks(context).map((block, i) => {
        switch (block.kind) {
          case "paragraph":
            return (
              <p key={i}>
                <Spans spans={block.spans} />
              </p>
            );
          case "heading":
            return (
              <p key={i} className="sheet-answer-heading">
                <Spans spans={block.spans} />
              </p>
            );
          case "list":
            return (
              <ul key={i}>
                {block.items.map((item, j) => (
                  <li key={j}>
                    <span className="sheet-answer-marker" aria-hidden>
                      {item.marker}
                    </span>
                    <span>
                      <Spans spans={item.spans} />
                    </span>
                  </li>
                ))}
              </ul>
            );
          case "code":
            return (
              <pre key={i} className="sheet-answer-pre">
                {block.text}
              </pre>
            );
          case "table":
            return (
              <p key={i} className="sheet-answer-table">
                {block.text}
              </p>
            );
        }
      })}
    </div>
  );
}

/** The question line and its context, one text that scrolls when its place caps it; the icon sits at its top. */
export function QuestionText({
  answer,
  textRef,
  cap = null,
}: {
  answer: SheetAnswer;
  textRef?: Ref<HTMLDivElement>;
  /** The text's cap in pixels, past which it scrolls; `null` for none of its own. */
  cap?: number | null;
}) {
  return (
    <div ref={textRef} className="sheet-answer-text" style={cap === null ? undefined : { maxHeight: cap }}>
      <div className="sheet-answer-question">
        <Icon name="message-circle-question-mark" size={16} />
        <div className="sheet-answer-words">
          <p className="sheet-answer-line">{answer.line}</p>
          {answer.context !== null && <QuestionContext context={answer.context} />}
        </div>
      </div>
    </div>
  );
}

/**
 * On a ticket's Overview the question box is first on the page and has no cap of its own: the page scrolls. Only when
 * Send would fall below the sheet does the question's text cap at half the sheet's body and scroll, so the options
 * and Send stay in view. Outside a sheet (the Gallery) nothing caps. `box` goes on the whole box, `text` on the
 * question's text, `cap` on the text.
 */
export function useSendInView(): {
  box: RefObject<HTMLDivElement | null>;
  text: RefObject<HTMLDivElement | null>;
  cap: number | null;
} {
  const box = useRef<HTMLDivElement>(null);
  const text = useRef<HTMLDivElement>(null);
  const [cap, setCap] = useState<number | null>(null);
  const measure = useCallback(() => {
    const b = box.current;
    const t = text.current;
    const body = b?.closest<HTMLElement>(".sheet-body");
    if (!b || !t || !body) return;
    // The box's height with its text whole, wherever the cap is now.
    const whole = b.offsetHeight - t.clientHeight + t.scrollHeight;
    const top = b.getBoundingClientRect().top - body.getBoundingClientRect().top + body.scrollTop;
    const next = top + whole <= body.clientHeight ? null : Math.floor(body.clientHeight / 2);
    setCap((current) => (current === next ? current : next));
  }, []);
  useLayoutEffect(() => {
    const b = box.current;
    const t = text.current;
    const body = b?.closest<HTMLElement>(".sheet-body");
    if (!b || !t || !body || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(body);
    observer.observe(b);
    if (t.firstElementChild) observer.observe(t.firstElementChild);
    return () => {
      observer.disconnect();
    };
  }, [measure]);
  // Measured once drawn too, before the first paint.
  useLayoutEffect(measure, [measure]);
  return { box, text, cap };
}
