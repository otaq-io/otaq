// Edit flow's box for Noyan, docked at the editor's foot under the columns (the board's box for new work, which Edit
// flow replaces, has the same look). It follows Noyan's conversation, one state at a time (`ask.state`):
// - change: "Ask Noyan to change this flow…" with Suggest a whole flow beside it, and under them who answers and what
//   it does. ↵ sends the typed words to Noyan's agent on the `flow` skill; the shortcut asks for a whole flow.
// - drafting: "Strong is drafting a change to shop-web's flow…" in the words' place, with Show (the agent panel on that
//   conversation) and Stop (it ends; the draft keeps what it proposed); the shortcut is off and says why. While the
//   agent waits on the user at a permission prompt, Needs you takes the line's place, with Answer.
// - asking: when the agent asks the user a question, the box is the answer box, in the attention colour: "Needs you ·
//   Strong asks", then the question and its answer UI as above Noyan's terminal (`AgentQuestionBox`), Answer in
//   terminal opening the agent panel. It grows with the question up to 55% of the editor's height, 760 px wide (680 px
//   in the narrowest window), and past that the question's text scrolls while the options and Send stay in view. ⤢
//   opens it up to the editor's full height over a scrim; ⤡, Esc (the editor's) or sending brings it back.
// - proposal: "Ask for changes to Strong's proposal…"; ↵ sends to the same conversation, and Show conversation opens
//   it. No shortcut.
// Sending doesn't open the agent panel: the proposal lands on the columns. Everything it says is `flow_board.view`'s
// `ask`.
import { type CSSProperties, type RefObject, useCallback, useLayoutEffect, useRef, useState } from "react";

import { IconButton, Kbd, PLAIN_TEXT, Tooltip } from "../../components";
import type { FlowBoardAsk } from "../../generated/FlowBoardAsk";
import type { FlowBoardAsking } from "../../generated/FlowBoardAsking";
import type { FlowBoardDrafting } from "../../generated/FlowBoardDrafting";
import { Icon } from "../../theme/Icon";
import { AgentQuestionBox } from "../agent_panel/AgentQuestion";
import { type AnswerBoxSize, answerBoxSize } from "./logic";

export interface NoyanBoxProps {
  ask: FlowBoardAsk;
  /**
   * The user's words, or `null` for Suggest a whole flow; while a proposal shows, the words ask for changes to it.
   * Resolves once sent; a refusal keeps the words.
   */
  onSend: (text: string | null) => Promise<unknown>;
  /** Show, Answer or Show conversation: the agent panel on that conversation. */
  onShow: (agent: string) => void;
  /** Stop: the drafting conversation ends. */
  onStop: (agent: string) => void;
  /** Noyan's question is open larger (⤢), over a scrim. */
  expanded?: boolean;
  /** ⤢ (`true`), or ⤡, a click on the scrim or the answer sent (`false`). */
  onExpand?: (open: boolean) => void;
}

const ignore = () => undefined;

export function NoyanBox({
  ask,
  onSend,
  onShow,
  onStop,
  expanded = false,
  onExpand = ignore,
}: NoyanBoxProps) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const typed = text.trim() !== "";
  const off = ask.off !== null;
  const state = ask.state;

  const send = (words: string | null) => {
    if (sending || off || words?.trim() === "") return;
    setSending(true);
    onSend(words).then(
      () => {
        setSending(false);
        if (words !== null) setText("");
      },
      () => {
        setSending(false);
      },
    );
  };

  const whole = ask.whole !== null && (state.kind === "drafting" || !typed) && (
    <Tooltip text={ask.off ?? ""}>
      <button
        type="button"
        className="fb-noyan-whole"
        disabled={off || sending}
        onClick={() => {
          send(null);
        }}
      >
        <Icon name="sparkles" size={13} />
        <span>{ask.whole}</span>
      </button>
    </Tooltip>
  );

  if (state.kind === "asking") {
    return <Asking asking={state} expanded={expanded} onExpand={onExpand} onShow={onShow} />;
  }

  if (state.kind === "drafting") {
    return (
      <div className="fb-noyan" data-state="drafting" role="group" aria-label={state.text}>
        <Drafting drafting={state} onShow={onShow} onStop={onStop}>
          {whole}
        </Drafting>
        <div className="fb-noyan-foot">
          <span className="fb-noyan-agent">{ask.agent}</span>
          <span className="fb-spacer" />
          <span className="fb-noyan-line">{ask.line}</span>
        </div>
      </div>
    );
  }

  return (
    <div
      className="fb-noyan"
      data-state={state.kind}
      data-typed={typed || undefined}
      role="group"
      aria-label={ask.placeholder}
    >
      <div className="fb-noyan-row">
        <Icon name={sending ? "loader-circle" : "bot"} size={15} />
        <input
          {...PLAIN_TEXT}
          className="fb-noyan-input"
          aria-label={ask.placeholder}
          placeholder={ask.placeholder}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || e.shiftKey || e.altKey || e.metaKey || e.ctrlKey) return;
            if (e.nativeEvent.isComposing) return;
            e.preventDefault();
            if (typed) send(text);
          }}
        />
        {whole}
      </div>
      <div className="fb-noyan-foot">
        <span className="fb-noyan-agent">{ask.agent}</span>
        <span className="fb-spacer" />
        {typed ? (
          <Tooltip text={ask.off ?? ""}>
            <button
              type="button"
              className="fb-noyan-send"
              disabled={off || sending}
              onClick={() => {
                send(text);
              }}
            >
              <Icon name="send-horizontal" size={13} />
              <span>{ask.send}</span>
            </button>
          </Tooltip>
        ) : (
          <>
            <span className="fb-noyan-line" data-off={off || undefined}>
              {ask.off ?? ask.line}
            </span>
            {state.kind === "proposal" && (
              <button
                type="button"
                className="fb-link"
                onClick={() => {
                  onShow(state.agent);
                }}
              >
                {state.show}
              </button>
            )}
          </>
        )}
        <Kbd keys={ask.keys} />
      </div>
    </div>
  );
}

/**
 * The editor's size (the area under the bar, `.fb-board`) as the answer box takes it: its width and cap. `null` until
 * measured (and where nothing lays out), when the box keeps its CSS width and has no cap.
 */
function useAnswerBoxSize(): { box: RefObject<HTMLDivElement | null>; size: AnswerBoxSize | null } {
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<AnswerBoxSize | null>(null);
  const measure = useCallback(() => {
    const editor = box.current?.closest<HTMLElement>(".fb-board");
    const next =
      editor && editor.clientHeight > 0 ? answerBoxSize(editor.clientWidth, editor.clientHeight) : null;
    setSize((current) => (current?.width === next?.width && current?.cap === next?.cap ? current : next));
  }, []);
  useLayoutEffect(() => {
    const editor = box.current?.closest<HTMLElement>(".fb-board");
    if (!editor || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(editor);
    return () => {
      observer.disconnect();
    };
  }, [measure]);
  // Measured once drawn too, before the first paint.
  useLayoutEffect(measure, [measure]);
  return { box, size };
}

/** The answer box: "Needs you · Strong asks" with ⤢ / ⤡, then Noyan's question and its answer UI. */
function Asking({
  asking,
  expanded,
  onExpand,
  onShow,
}: {
  asking: FlowBoardAsking;
  expanded: boolean;
  onExpand: (open: boolean) => void;
  onShow: (agent: string) => void;
}) {
  const { box, size } = useAnswerBoxSize();
  // In the box it caps at 55% of the editor; open larger, the CSS gives it the editor's height.
  let style: CSSProperties | undefined;
  if (size !== null) style = expanded ? { width: size.width } : { width: size.width, maxHeight: size.cap };
  const { question } = asking;
  const toggle = expanded ? asking.collapse : asking.expand;
  return (
    <>
      {expanded && (
        <div
          className="fb-noyan-scrim"
          role="presentation"
          onMouseDown={(e) => {
            e.preventDefault();
            onExpand(false);
          }}
        />
      )}
      <div
        ref={box}
        className="fb-noyan"
        data-state="asking"
        data-expanded={expanded || undefined}
        role="group"
        aria-label={asking.head}
        style={style}
      >
        <div className="fb-noyan-head">
          <strong className="fb-needs-you">{asking.head}</strong>
          <span className="fb-spacer" />
          <Tooltip text={toggle}>
            <IconButton
              icon={expanded ? "minimize-2" : "maximize-2"}
              label={toggle}
              onClick={() => {
                onExpand(!expanded);
              }}
            />
          </Tooltip>
        </div>
        <AgentQuestionBox
          key={question.answer.question}
          question={question}
          onTerminal={() => {
            onShow(question.agent);
          }}
          onSent={() => {
            onExpand(false);
          }}
        />
      </div>
    </>
  );
}

/**
 * The drafting row, in the words' place: what the agent does (or, waiting on the user, Needs you and what it waits on,
 * one line with the whole of it in the tooltip), Show or Answer, Stop, then the shortcut (off) the box passes in.
 */
function Drafting({
  drafting,
  onShow,
  onStop,
  children,
}: {
  drafting: FlowBoardDrafting;
  onShow: (agent: string) => void;
  onStop: (agent: string) => void;
  children: React.ReactNode;
}) {
  const waiting = drafting.needs_you !== null;
  const text = <span className="fb-drafting-text">{drafting.text}</span>;
  return (
    <div className="fb-noyan-row fb-drafting" role="status" data-needs-you={waiting || undefined}>
      <Icon name={waiting ? "hand" : "loader-circle"} size={15} />
      {waiting && <strong className="fb-needs-you">{drafting.needs_you}</strong>}
      {drafting.tooltip !== null ? <Tooltip text={drafting.tooltip}>{text}</Tooltip> : text}
      <span className="fb-spacer" />
      <button
        type="button"
        className="fb-link"
        onClick={() => {
          onShow(drafting.agent);
        }}
      >
        {drafting.show}
      </button>
      <button
        type="button"
        className="fb-link"
        onClick={() => {
          onStop(drafting.agent);
        }}
      >
        {drafting.stop}
      </button>
      {children}
    </div>
  );
}
