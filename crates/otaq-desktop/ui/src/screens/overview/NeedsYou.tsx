// The top of the sheet's Overview, what the ticket needs from you (SHT-03, T-0465; UI-04, ATT-12, ATT-17): the action
// bar of the ticket's state, or the answer options of an Asks-you question; the held roles' callouts, the refusal, the
// queued notice, the conflict strip and the Start interrupted callout. The stage's agents are set in Edit ticket… (no
// Path, no locks). The M12 rows (d0063,
// d0064): the validation buttons, the token split, the Auto-merge stop message and Answer the agent's note box. The
// actions are the sheet's (`sheet/actions`); everything it says comes from `sheet.view`; it keeps only the picked
// answer. At Review, "Manual checks for you" (the Overview's, `aboveBar`) sits above Approve, and a build set aside to
// save disk says so under the bar (d0125, `BuildAside`; the Run and Shell show the same line).
import {
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useEffect,
  useEffectEvent,
  useId,
  useRef,
  useState,
} from "react";

import { openOverlay } from "../../app/overlays";
import { useRegistries } from "../../app/registry";
import { SHEET_UNDER_TABS } from "../../app/registry/sheet";
import { requestTerminalFocus, showTab } from "../../app/selection";
import { Button, PROSE_TEXT, TextArea, ToolProblemCallout, Tooltip } from "../../components";
import type { BuildAsideLine } from "../../generated/BuildAsideLine";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import type { SheetAnswerAgent } from "../../generated/SheetAnswerAgent";
import type { SheetButton } from "../../generated/SheetButton";
import type { SheetButtonLook } from "../../generated/SheetButtonLook";
import type { SheetNotice } from "../../generated/SheetNotice";
import type { SheetPermissionPrompt } from "../../generated/SheetPermissionPrompt";
import type { SheetSent } from "../../generated/SheetSent";
import type { SheetHookRefusal } from "../../generated/SheetHookRefusal";
import type { SheetRefusal } from "../../generated/SheetRefusal";
import type { SheetStartProblem } from "../../generated/SheetStartProblem";
import type { SheetRunPill } from "../../generated/SheetRunPill";
import type { SheetView } from "../../generated/SheetView";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { askUnsent, type CardAnswer, type SheetActions } from "../sheet/actions";
import { HookOutput } from "../sheet/HookOutput";
import { canSend, initialPicks, pickByKey, picksOther, pressPick } from "../sheet/actionRoutes";
import { toolProblemAction } from "../tool_problems/toolProblemRunner";
import { useAnswerDraft } from "./answerDraft";
import { QuestionText, useSendInView } from "./QuestionText";

const ignore = () => undefined;

const LOOK_VARIANT = {
  primary: "primary",
  secondary: "secondary",
  attention: "primary",
  danger: "destructive",
} as const satisfies Record<SheetButtonLook, string>;

function ActionButton({
  button,
  busy,
  onPress,
}: {
  button: SheetButton;
  busy: boolean;
  onPress: (button: SheetButton) => void;
}) {
  const attention = button.look === "attention";
  const pressable = (
    <Button
      label={button.label}
      variant={LOOK_VARIANT[button.look]}
      icon={attention ? "corner-down-right" : (button.icon ?? undefined)}
      loading={busy}
      onClick={() => {
        onPress(button);
      }}
    />
  );
  return (
    <span className="sheet-action" data-look={button.look}>
      {button.tooltip === null ? pressable : <Tooltip text={button.tooltip}>{pressable}</Tooltip>}
    </span>
  );
}

/** What is picked in an answer box, and its ways to change it: one pick, or, on a question that takes several answers,
 * every ticked row. Opens on the recommended ones. Noyan's box keeps its picks here; a ticket's keeps them in its draft
 * (`answerDraft`). */
export function useAnswerPicks(answer: SheetAnswer): {
  picks: number[];
  /** A click or number key: picks the row, or ticks / unticks it on a question that takes several answers. */
  press: (n: number) => void;
  /** A card's option: the row is picked (ticked, never unticked). */
  pickRow: (n: number) => void;
} {
  const [picks, setPicks] = useState(() => initialPicks(answer.rows));
  const multiple = answer.multiple === true;
  return {
    picks,
    press: (n) => {
      setPicks((now) => pressPick(now, n, multiple));
    },
    pickRow: (n) => {
      setPicks((now) => (now.includes(n) ? now : pressPick(now, n, multiple)));
    },
  };
}

/** The picks a request carries: `pick` (the one pick, as before), and every ticked row in `picks` when the question
 * takes several answers. */
export function picksParams(
  answer: SheetAnswer,
  picks: readonly number[],
): { pick?: number; picks?: number[] } {
  const pick = picks.at(0);
  return {
    ...(pick !== undefined && { pick }),
    ...(answer.multiple === true && { picks: [...picks] }),
  };
}

/**
 * The question and its numbered options (ATT-12, ATT-17), each with its note, the recommended ones marked, and
 * Other…'s field: a ticket's question bar and a Project agent's question (OTQ-9) draw it. 1–4 pick, ↵ sends. A question
 * that takes several answers draws checkboxes instead of radios: a click or number key ticks or unticks its row, Other…
 * too.
 */
export function AnswerChoices({
  answer,
  picks,
  other,
  onPick,
  onOther,
  onSend,
  options,
  textRef,
  cap = null,
}: {
  answer: SheetAnswer;
  picks: readonly number[];
  other: string;
  onPick: (pick: number) => void;
  onOther: (other: string) => void;
  onSend: () => void;
  options?: Ref<HTMLDivElement>;
  /** The question's text: the part that scrolls when its place caps it. */
  textRef?: Ref<HTMLDivElement>;
  /** The text's cap in pixels, when its place sets one. */
  cap?: number | null;
}) {
  const noteId = useId();
  const multiple = answer.multiple === true;
  return (
    <div className="sheet-answer">
      <QuestionText answer={answer} textRef={textRef} cap={cap} />
      <div
        ref={options}
        className="sheet-answer-options"
        role="radiogroup"
        // Checkboxes sit in a group: several may be ticked.
        {...(multiple && { role: "group" })}
        aria-label={answer.line}
        // Its number keys pick or tick, before the sheet's 1–4 tabs (`keys.ts`), checkboxes as radios.
        data-number-keys
        tabIndex={-1}
        onKeyDown={(e) => {
          // A key typed in a text field (Other…'s) is the field's own: its digits type, its ↵ sends from there.
          if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
          const n = pickByKey(e.key, answer.rows);
          if (n !== null) {
            e.preventDefault();
            onPick(n);
          } else if (e.key === "Enter") {
            e.preventDefault();
            onSend();
          }
        }}
      >
        {answer.rows.map((row) => (
          <button
            key={row.number}
            type="button"
            role={multiple ? "checkbox" : "radio"}
            aria-checked={picks.includes(row.number)}
            aria-describedby={row.note === null ? undefined : `${noteId}-${String(row.number)}`}
            className="sheet-answer-option"
            data-picked={picks.includes(row.number) || undefined}
            data-other={row.other || undefined}
            onClick={() => {
              onPick(row.number);
            }}
          >
            {multiple ? (
              <span className="sheet-answer-check">
                {picks.includes(row.number) && <Icon name="check" size={11} />}
              </span>
            ) : (
              <span className="sheet-answer-radio" />
            )}
            <span className="sheet-answer-label">
              {row.label}
              {/* The option's note (ATT-17), whole and wrapped: its description. */}
              {row.note !== null && (
                <span id={`${noteId}-${String(row.number)}`} className="sheet-answer-note" aria-hidden>
                  {row.note}
                </span>
              )}
            </span>
            {row.recommended && (
              <span className="sheet-answer-rec">
                <Icon name="star" size={10} />
                {answer.recommended_label}
              </span>
            )}
            <span className="sheet-answer-number">{row.number}</span>
          </button>
        ))}
        {picksOther(picks, answer.rows) && (
          <textarea
            {...PROSE_TEXT}
            className="text-input text-area sheet-answer-box"
            rows={2}
            value={other}
            placeholder={answer.other_placeholder}
            aria-label={answer.other_placeholder}
            // The field appears because Other… was just picked: typing goes straight into it.
            // eslint-disable-next-line jsx-a11y/no-autofocus -- it opens on the user's own pick
            autoFocus
            onChange={(e) => {
              onOther(e.target.value);
            }}
            onKeyDown={(e) => {
              sendOnEnter(e, onSend);
            }}
          />
        )}
      </div>
    </div>
  );
}

/** An answer box's keys: ↵ sends, ⇧↵ starts a new line (and ↵ while an input method composes stays its own). */
export function sendOnEnter(e: KeyboardEvent<HTMLTextAreaElement>, onSend: () => void): void {
  if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
  e.preventDefault();
  onSend();
}

/** An open question (no options): the question, then a few lines' box for the user's own words; ↵ sends, ⇧↵ a new
 * line. */
function OpenAnswer({
  answer,
  text,
  onText,
  onSend,
  textRef,
  cap = null,
}: {
  answer: SheetAnswer;
  text: string;
  onText: (text: string) => void;
  onSend: () => void;
  textRef?: Ref<HTMLDivElement>;
  /** The text's cap in pixels, when its place sets one. */
  cap?: number | null;
}) {
  return (
    <div className="sheet-answer">
      <QuestionText answer={answer} textRef={textRef} cap={cap} />
      <textarea
        {...PROSE_TEXT}
        className="text-input text-area sheet-answer-box"
        rows={3}
        value={text}
        placeholder={answer.other_placeholder}
        aria-label={answer.other_placeholder}
        onChange={(e) => {
          onText(e.target.value);
        }}
        onKeyDown={(e) => {
          sendOnEnter(e, onSend);
        }}
      />
    </div>
  );
}

/** An answer box's sending, one at a time: `once` runs a send only when none is going. Kept in a ref, not state, so
 * two calls in the same event (or before the next render) send once; `sending` draws the button's spinner. */
export function useSendOnce(): { sending: boolean; once: (go: () => Promise<unknown>) => Promise<unknown> } {
  const going = useRef(false);
  const [sending, setSending] = useState(false);
  return {
    sending,
    once: (go) => {
      if (going.current) return Promise.resolve();
      going.current = true;
      setSending(true);
      return go().finally(() => {
        going.current = false;
        setSending(false);
      });
    },
  };
}

/** Whether an answer can go: an open question's words, else a pick or at least one ticked row (and Other…'s text). */
export function canAnswerQuestion(answer: SheetAnswer, picks: readonly number[], other: string): boolean {
  if (answer.rows.length === 0) return other.trim() !== "";
  return canSend(picks, other, answer.rows);
}

/** The card clicks an answer box has taken: each click is its own object. */
const cardsTaken = new WeakSet<CardAnswer>();

/** The question bar with its options, or an open question's box (ATT-12, ATT-17); the Gallery draws it too. */
export function AnswerBox({
  view,
  answer,
  more,
  fromCard,
}: {
  view: SheetView;
  answer: SheetAnswer;
  more: ReactNode;
  fromCard: CardAnswer | null;
}) {
  // The picks and Something else's text outlive the box: Overview unmounts while another tab shows.
  const { picks, other, press, pickRow, setOther, sent: answered } = useAnswerDraft(view.key, answer);
  const options = useRef<HTMLDivElement>(null);
  const { box, text, cap, boxCap } = useSendInView();
  const { sending, once } = useSendOnce();
  const open = answer.rows.length === 0;
  const ready = canAnswerQuestion(answer, picks, other);
  const send = (clear = false): Promise<unknown> => {
    if (!ready) return Promise.resolve();
    // An open question's words go as its answer; the pick is the options' only.
    const params = {
      key: view.key,
      question: answer.question,
      pick: 1,
      ...picksParams(answer, picks),
      other,
      clear,
    };
    return once(() =>
      (request("sheet.answer", params) as Promise<SheetSent>)
        .then((sent) => {
          if (sent.outcome === "done") answered();
          askUnsent(sent, () => send(true));
        })
        .catch(ignore),
    );
  };
  // A card's option opens the list with it picked (sending stays one more click or ↵, ATT-12); its Send answer
  // sends the picked (recommended) option.
  const onCard = useEffectEvent((card: CardAnswer) => {
    if (card.question !== answer.question) return;
    // The sheet keeps the card's last click; the box comes back with each return to Overview and must not pick it
    // over what was picked since.
    if (cardsTaken.has(card)) return;
    cardsTaken.add(card);
    if (card.kind === "send_answer") {
      void send();
    } else if (answer.rows.some((r) => r.number === card.number)) {
      pickRow(card.number);
      options.current?.focus();
    }
  });
  useEffect(() => {
    // The card's click arrives from outside React, once.
    if (fromCard !== null) onCard(fromCard);
  }, [fromCard]);
  return (
    <div ref={box} className="sheet-question" style={boxCap === null ? undefined : { maxHeight: boxCap }}>
      {open ? (
        <OpenAnswer
          answer={answer}
          textRef={text}
          cap={cap}
          text={other}
          onText={setOther}
          onSend={() => {
            void send();
          }}
        />
      ) : (
        <AnswerChoices
          answer={answer}
          picks={picks}
          other={other}
          onPick={press}
          onOther={setOther}
          onSend={() => {
            void send();
          }}
          options={options}
          textRef={text}
          cap={cap}
        />
      )}
      <div className="sheet-bar">
        <span className="sheet-answer-hint">{answer.hint}</span>
        <Button
          label={answer.in_terminal}
          variant="ghost"
          icon="square-terminal"
          onClick={() => {
            showTab("agent");
            requestTerminalFocus(view.key);
          }}
        />
        <Button
          label={answer.send}
          icon="send"
          disabled={!ready}
          loading={sending}
          onClick={() => {
            void send();
          }}
        />
        {more}
      </div>
    </div>
  );
}

/** *SHOP-32 · Validation failed 3 times*: the note goes with the last findings (`sheet.answer_agent`). */
function AnswerAgentBox({
  view,
  box,
  onClose,
}: {
  view: SheetView;
  box: SheetAnswerAgent;
  onClose: () => void;
}) {
  const [note, setNote] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const send = () => {
    if (sending || note.trim() === "") return;
    setSending(true);
    setError(null);
    request("sheet.answer_agent", { key: view.key, note }).then(onClose, (e: unknown) => {
      setError(asRequestError(e).message);
      setSending(false);
    });
  };
  return (
    <div className="sheet-answer-agent">
      <span className="sheet-answer-agent-heading">{box.heading}</span>
      <TextArea id="sheet-answer-agent-note" value={note} placeholder={box.placeholder} onChange={setNote} />
      <p className="sheet-answer-agent-footer">{box.footer}</p>
      {error !== null && <p className="sheet-dialog-error">{error}</p>}
      <div className="sheet-answer-agent-buttons">
        <Button variant="ghost" icon="check" label={box.cancel} onClick={onClose} />
        <Button icon="send" label={box.send} disabled={note.trim() === ""} loading={sending} onClick={send} />
      </div>
    </div>
  );
}

function Notice({ view, notice }: { view: SheetView; notice: SheetNotice }) {
  return (
    <div className="sheet-notice" data-closed={notice.parent_closed || undefined}>
      <Icon name={notice.parent_closed ? "circle-slash" : "hourglass"} size={13} />
      <div className="sheet-notice-body">
        <p>{notice.text}</p>
        {notice.choices.length > 0 && (
          <div className="sheet-notice-choices">
            {notice.choices.map((choice) => (
              <span key={choice.parent} className="sheet-notice-pair">
                <Button
                  compact
                  variant="secondary"
                  icon="rotate-ccw"
                  label={choice.reopen}
                  onClick={() => {
                    request("sheet.reopen_parent", { key: view.key, parent: choice.parent }).catch(ignore);
                  }}
                />
                <Button
                  compact
                  variant="secondary"
                  label={choice.remove}
                  onClick={() => {
                    openOverlay("confirm", {
                      title: choice.confirm_title,
                      body: choice.confirm_body,
                      confirmLabel: choice.confirm_label,
                      destructive: false,
                      onConfirm: () =>
                        request("sheet.remove_dependency", { key: view.key, parent: choice.parent }),
                    });
                  }}
                />
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** A permission prompt above Answer in terminal: "Developer wants to run a command" and the command (`$ npm
 * install`), or "Developer wants your permission" and the tool's own words. Otaq doesn't answer it: the answer goes
 * in the terminal. */
function PermissionPrompt({ prompt }: { prompt: SheetPermissionPrompt }) {
  return (
    <div className="sheet-permission">
      <div className="sheet-permission-heading">
        <Icon name="hand" size={14} />
        <span>{prompt.heading}</span>
      </div>
      {prompt.command ? (
        <code className="sheet-permission-command">
          <span className="sheet-permission-dollar" aria-hidden>
            $
          </span>
          {prompt.text}
        </code>
      ) : (
        <p className="sheet-permission-text">{prompt.text}</p>
      )}
    </div>
  );
}

/** A refused model above the bar (AGT-05, *05 → Start failed on the model*, *No result: the validator's model*): what
 * couldn't run (red for Start failed) and the tool's message in quotes. */
function Refusal({ refusal }: { refusal: SheetRefusal }) {
  return (
    <div className="sheet-refusal" data-error={refusal.error || undefined}>
      <span className="sheet-refusal-heading">{refusal.heading}</span>
      <span className="sheet-refusal-message">{refusal.message}</span>
    </div>
  );
}

/** A project's git hook refused Approve & merge or a PR's commit message, above the bar whose primary button is the way
 * on (Edit message(s) and retry): which hook and step, the refused message, the hook's output in the same box as a
 * refused start's, the way on. */
export function HookRefusal({ refusal }: { refusal: SheetHookRefusal }) {
  return (
    <div className="sheet-hook-refusal" role="alert">
      <div className="sheet-hook-refusal-heading">
        <Icon name="circle-alert" size={14} />
        <span>{refusal.heading}</span>
      </div>
      {refusal.step !== null && <p className="sheet-hook-refusal-text">{refusal.step}</p>}
      {refusal.message !== null && (
        <div className="sheet-hook-refusal-message">
          <span className="sheet-hook-refusal-message-label">{refusal.message.label}</span>
          <pre className="sheet-hook-refusal-message-text">{refusal.message.text}</pre>
        </div>
      )}
      <HookOutput hook={refusal.hook} />
      <p className="sheet-hook-refusal-text">{refusal.way_on}</p>
    </div>
  );
}

function StartProblem({ view, problem }: { view: SheetView; problem: SheetStartProblem }) {
  const [log, setLog] = useState<string | null>(null);
  const [shown, setShown] = useState(false);
  return (
    <div className="sheet-start-problem">
      <div className="sheet-start-callout">
        <div className="sheet-start-heading">
          <Icon name="circle-alert" size={14} />
          <span>{problem.heading}</span>
        </div>
        <p className="sheet-start-message">{problem.message}</p>
        {problem.has_log && (
          <button
            type="button"
            className="sheet-link"
            onClick={() => {
              if (!shown && log === null) {
                (request("sheet.setup_log", { key: view.key }) as Promise<string | null>).then(
                  setLog,
                  ignore,
                );
              }
              setShown(!shown);
            }}
          >
            {shown ? problem.hide_log : problem.show_log}
          </button>
        )}
      </div>
      {shown && log !== null && <pre className="sheet-setup-log">{log}</pre>}
      {problem.hook !== null && <HookOutput hook={problem.hook} />}
      <div className="sheet-start-does">
        <span className="sheet-start-does-heading">{problem.retry_heading}</span>
        <ul>
          {problem.retry_does.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/**
 * The held roles' callouts above the bar (§7.2, SCH-04 `[M23]`; *05 → Column 3 · several tools (M23)*): "Model
 * unavailable", "Codex isn't available · hidden in Settings", "Claude Code limit reached · resets 3:45 pm", with Open
 * agent…, Sign in… and Show in Agent tools.
 */
function Holds({ view }: { view: SheetView }) {
  if (view.holds.length === 0) return null;
  return (
    <div className="sheet-holds">
      {view.holds.map((hold) => (
        <ToolProblemCallout
          key={`${hold.role ?? ""}${hold.heading}`}
          callout={hold}
          look="edge"
          onAction={(action) => {
            toolProblemAction(action, { ticket: view.key });
          }}
        />
      ))}
    </div>
  );
}

/** The Run's pill while it is up (Open, Stop). The last signal and the tokens stay out of Overview: History has each
 * session's tokens. */
function MetaLine({ view }: { view: SheetView }) {
  if (view.run === null) return null;
  return (
    <div className="sheet-meta">
      <RunPill ticket={view.key} pill={view.run} />
    </div>
  );
}

/**
 * The top of the sheet's Overview (SHT-03's "what it needs from you", T-0465): the old panel header's lower part (the
 * conflict strip, the question with its options or the action bar with its callouts, the Auto-merge stop message,
 * Answer the agent's box), the queued notice, the open PR's box and the Start interrupted
 * callout. The registered sections (Stack, Messages) are Overview's own, under the description.
 */
/** "The build was set aside to save disk · Run or Shell brings it back", or "Bringing the build back…" (d0125). */
export function BuildAside({ line }: { line: BuildAsideLine }) {
  return (
    <p className="build-aside" data-bringing={line.bringing || undefined} role="status">
      <Icon name={line.icon} size={13} />
      <span>{line.text}</span>
    </p>
  );
}

export function NeedsYou({
  view,
  actions,
  aboveBar,
}: {
  view: SheetView;
  actions: SheetActions;
  /** Shown right above the action bar: the review's manual checks (d0125). */
  aboveBar?: ReactNode;
}) {
  const app = useRegistries();
  const { press, busy, answering, setAnswering, cardAnswer } = actions;
  const UnderTabs = app.sheetSections.get(SHEET_UNDER_TABS)?.Component;
  return (
    <div className="sheet-overview">
      <div className="sheet-overview-header">
        <MetaLine view={view} />
        {view.conflict && (
          <div className="sheet-conflict">
            <Icon name="triangle-alert" size={13} />
            <div className="sheet-conflict-text">
              <span className="sheet-conflict-line">{view.conflict.line}</span>
              {view.conflict.moved !== null && (
                <span className="sheet-conflict-more">{view.conflict.moved}</span>
              )}
              {view.conflict.outcome !== null && (
                <span className="sheet-conflict-more">{view.conflict.outcome}</span>
              )}
            </div>
            {view.conflict.count !== null && (
              <Tooltip text={view.conflict.count_hint}>
                <span className="sheet-conflict-count">{view.conflict.count}</span>
              </Tooltip>
            )}
          </div>
        )}
        {view.answer ? (
          <AnswerBox
            key={view.answer.question}
            view={view}
            answer={view.answer}
            more={null}
            fromCard={cardAnswer}
          />
        ) : (
          <>
            {view.permission_prompt && <PermissionPrompt prompt={view.permission_prompt} />}
            {view.refusal && <Refusal refusal={view.refusal} />}
            {view.hook_refusal && <HookRefusal refusal={view.hook_refusal} />}
            <Holds view={view} />
            {aboveBar}
            {view.buttons.length > 0 && (
              <div className="sheet-bar">
                {view.buttons.map((button) => (
                  <span key={button.action} className="sheet-action-group">
                    <ActionButton button={button} busy={busy === button.action} onPress={press} />
                    {button.action === "approve" && view.approve_more !== null && (
                      <Tooltip text={view.approve_more}>
                        <Button
                          label="▾"
                          onClick={() => {
                            openOverlay("local_merge", { ticket: view.key, menu: true });
                          }}
                        />
                      </Tooltip>
                    )}
                  </span>
                ))}
              </div>
            )}
          </>
        )}
        {/* A build set aside to save disk says so where it is brought back: Run and Terminal, not here. */}
        {view.stop_message !== null && (
          <p className="sheet-stop-message">
            <Icon name="triangle-alert" size={13} />
            <span>{view.stop_message}</span>
          </p>
        )}
        {answering === view.key && view.answer_agent && (
          <AnswerAgentBox
            key={view.key}
            view={view}
            box={view.answer_agent}
            onClose={() => {
              setAnswering(null);
            }}
          />
        )}
      </div>
      {view.notice && <Notice view={view} notice={view.notice} />}
      {UnderTabs && <UnderTabs ticket={view.key} />}
      {view.start_problem && <StartProblem key={view.key} view={view} problem={view.start_problem} />}
    </div>
  );
}

/** "● :4020 ↗ ▢" while the Run is up (*13 → Card & sheet-overview-header controls*): Open (with an Open URL) and Stop. */
function RunPill({ ticket, pill }: { ticket: string; pill: SheetRunPill }) {
  return (
    <span className="sheet-run">
      <span className="sheet-run-dot" />
      <button
        type="button"
        className="sheet-run-port"
        onClick={() => {
          showTab("run");
        }}
      >
        {pill.port}
      </button>
      {pill.url !== null && (
        <Tooltip text={pill.open_hint}>
          <button
            type="button"
            className="sheet-run-icon"
            aria-label={pill.open_hint}
            onClick={() => {
              request("run.open", { url: pill.url }).catch(ignore);
            }}
          >
            <Icon name="external-link" size={11} />
          </button>
        </Tooltip>
      )}
      <Tooltip text={pill.stop_hint}>
        <button
          type="button"
          className="sheet-run-icon"
          aria-label={pill.stop_hint}
          onClick={() => {
            request("run.stop", { key: ticket }).catch(ignore);
          }}
        >
          <Icon name="square" size={11} />
        </button>
      </Tooltip>
    </span>
  );
}
