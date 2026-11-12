// A Project agent's open question (OTQ-9; d0119, d0068, d0061, d0048): what Noyan asked with `otaq ask human`, in its
// chip's popover in the Needs you strip and above its terminal in the panel. The same answer UI as a ticket's question
// (`AnswerChoices`): its options with their notes and the recommended one picked, or a free-text box for an open
// question. Send answer calls `agent_panel.answer` (`project_agent.answer`); unsent text in its terminal asks "Keep it
// and cancel / Clear it and send" first. Everything it says is `AgentQuestion`'s.
import { useState } from "react";

import { Button, PLAIN_TEXT } from "../../components";
import type { AgentAnswerParams } from "../../generated/AgentAnswerParams";
import type { AgentQuestion } from "../../generated/AgentQuestion";
import type { SheetSent } from "../../generated/SheetSent";
import { request } from "../../tauri";
import { AnswerChoices } from "../overview/NeedsYou";
import { QuestionText } from "../overview/QuestionText";
import { askUnsent } from "../sheet/actions";
import { canSend, initialPick } from "../sheet/actionRoutes";

const ignore = () => undefined;

/** Whether Send answer can send: a pick (and Other…'s text), or an open question's text. */
export function canAnswer(question: AgentQuestion, pick: number | null, other: string): boolean {
  if (question.answer.rows.length === 0) return other.trim() !== "";
  return canSend(pick, other, question.answer.rows);
}

/** The `agent_panel.answer` request of a pick (none for an open question). */
export function answerParams(
  question: AgentQuestion,
  pick: number | null,
  other: string,
  clear: boolean,
): AgentAnswerParams {
  const open = question.answer.rows.length === 0;
  return {
    project: question.project,
    agent: question.agent,
    question: question.answer.question,
    ...(!open && pick !== null && { pick }),
    other,
    clear,
  };
}

export interface AgentQuestionBoxProps {
  question: AgentQuestion;
  /** Answer in terminal: the panel focuses its terminal, the strip opens the panel. */
  onTerminal: () => void;
  /** After the answer went (the popover closes). */
  onSent?: () => void;
  /** Draws the title ("Noyan · Checkout slowness asks") above the question: the strip's popover. */
  titled?: boolean;
}

/** The question with its answer UI, and Answer in terminal / Send answer. */
export function AgentQuestionBox({ question, onTerminal, onSent, titled = false }: AgentQuestionBoxProps) {
  const { answer } = question;
  const open = answer.rows.length === 0;
  const [pick, setPick] = useState(() => initialPick(answer.rows));
  const [other, setOther] = useState("");
  const [sending, setSending] = useState(false);
  const ready = canAnswer(question, pick, other);
  const send = (clear = false): Promise<unknown> => {
    if (!ready || sending) return Promise.resolve();
    setSending(true);
    return (request("agent_panel.answer", answerParams(question, pick, other, clear)) as Promise<SheetSent>)
      .then((sent) => {
        if (sent.outcome === "done") onSent?.();
        askUnsent(sent, () => send(true));
      })
      .catch(ignore)
      .finally(() => {
        setSending(false);
      });
  };
  return (
    <div className="agent-question" role="group" aria-label={question.title}>
      {titled && <div className="agent-question-title">{question.title}</div>}
      {open ? (
        <div className="sheet-answer">
          <QuestionText answer={answer} />
          <input
            {...PLAIN_TEXT}
            className="field-input agent-question-text"
            value={other}
            placeholder={answer.other_placeholder}
            aria-label={answer.other_placeholder}
            onChange={(e) => {
              setOther(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                void send();
              }
            }}
          />
        </div>
      ) : (
        <AnswerChoices
          answer={answer}
          pick={pick}
          other={other}
          onPick={setPick}
          onOther={setOther}
          onSend={() => {
            void send();
          }}
        />
      )}
      <div className="agent-question-bar">
        <span className="sheet-answer-hint">{answer.hint}</span>
        <span className="agent-question-spacer" />
        <Button
          compact
          label={answer.in_terminal}
          variant="secondary"
          icon="square-terminal"
          onClick={onTerminal}
        />
        <Button
          compact
          label={answer.send}
          icon="send"
          disabled={!ready}
          loading={sending}
          onClick={() => {
            void send();
          }}
        />
      </div>
    </div>
  );
}
