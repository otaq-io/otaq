// Edit flow's box for Noyan, docked at the editor's foot under the columns (the board's box for new work, which Edit
// flow replaces, has the same look): "Ask Noyan to change this flow…" with Suggest a whole flow beside it, and under
// them who answers and what it does. ↵ sends the typed words to Noyan's agent on the `flow` skill; the shortcut asks
// for a whole flow. Sending doesn't open the agent panel: the proposal lands on the columns. While Noyan drafts,
// neither can be asked, and the box says why. Everything it says is `flow_board.view`'s `ask`.
import { useState } from "react";

import { Kbd, PLAIN_TEXT, Tooltip } from "../../components";
import type { FlowBoardAsk } from "../../generated/FlowBoardAsk";
import { Icon } from "../../theme/Icon";

export interface NoyanBoxProps {
  ask: FlowBoardAsk;
  /** The user's words, or `null` for Suggest a whole flow. Resolves once sent; a refusal keeps the words. */
  onSend: (text: string | null) => Promise<unknown>;
}

/** The box in its asking state: the user types a request, or picks the whole-flow shortcut. */
export function NoyanBox({ ask, onSend }: NoyanBoxProps) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const typed = text.trim() !== "";
  const off = ask.off !== null;

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

  return (
    <div className="fb-noyan" data-typed={typed || undefined} role="group" aria-label={ask.placeholder}>
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
        {!typed && (
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
        )}
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
          <span className="fb-noyan-line" data-off={off || undefined}>
            {ask.off ?? ask.line}
          </span>
        )}
        <Kbd keys={ask.keys} />
      </div>
    </div>
  );
}
