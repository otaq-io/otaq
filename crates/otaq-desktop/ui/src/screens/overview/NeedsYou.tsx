// The top of the sheet's Overview, what the ticket needs from you (SHT-03, T-0465; UI-04, ATT-12, ATT-17): the action
// bar of the ticket's state, or the answer options of an Asks-you question; the held roles' callouts, the refusal, the
// queued notice, the conflict strip and the Start interrupted callout; the choices and the path. The M12 rows (d0063,
// d0064): the validation buttons, the token split, the Auto-merge stop message and Answer the agent's note box. The
// actions are the sheet's (`sheet/actions`); everything it says comes from `panel.view`; it keeps only the picked
// answer. At Review, "Manual checks for you" (the Overview's, `aboveBar`) sits above Approve, and a build set aside to
// save disk says so under the bar (d0125, `BuildAside`; the Run and Shell show the same line).
import { type ReactNode, useEffect, useEffectEvent, useId, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { useRegistries } from "../../app/registry";
import { PANEL_UNDER_TABS } from "../../app/registry/panel";
import { requestTerminalFocus, showTab } from "../../app/selection";
import {
  Button,
  StageChoices,
  TextArea,
  TicketPath,
  ToolProblemCallout,
  Tooltip,
  PLAIN_TEXT,
} from "../../components";
import type { BuildAsideLine } from "../../generated/BuildAsideLine";
import type { PanelAnswer } from "../../generated/PanelAnswer";
import type { PanelAnswerAgent } from "../../generated/PanelAnswerAgent";
import type { PanelButton } from "../../generated/PanelButton";
import type { PanelButtonLook } from "../../generated/PanelButtonLook";
import type { PanelChoiceParams } from "../../generated/PanelChoiceParams";
import type { PanelNotice } from "../../generated/PanelNotice";
import type { PanelSent } from "../../generated/PanelSent";
import type { PanelRefusal } from "../../generated/PanelRefusal";
import type { PanelStartProblem } from "../../generated/PanelStartProblem";
import type { PanelRunPill } from "../../generated/PanelRunPill";
import type { PanelView } from "../../generated/PanelView";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { LeadAsks } from "../epic/EpicSheet";
import { askUnsent, type CardAnswer, type PanelActions } from "../sheet/actions";
import { canSend, initialPick, pickByKey, picksOther } from "../sheet/actionRoutes";
import { toolProblemAction } from "../tool_problems/toolProblemRunner";

const ignore = () => undefined;

const LOOK_VARIANT = {
  primary: "primary",
  secondary: "secondary",
  attention: "primary",
  danger: "destructive",
} as const satisfies Record<PanelButtonLook, string>;

function ActionButton({
  button,
  busy,
  onPress,
}: {
  button: PanelButton;
  busy: boolean;
  onPress: (button: PanelButton) => void;
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
    <span className="panel-action" data-look={button.look}>
      {button.tooltip === null ? pressable : <Tooltip text={button.tooltip}>{pressable}</Tooltip>}
    </span>
  );
}

/** The question bar with its options (ATT-12, ATT-17); the Gallery draws it too. */
export function AnswerBox({
  view,
  answer,
  more,
  fromCard,
}: {
  view: PanelView;
  answer: PanelAnswer;
  more: ReactNode;
  fromCard: CardAnswer | null;
}) {
  const [pick, setPick] = useState(() => initialPick(answer.rows));
  const options = useRef<HTMLDivElement>(null);
  const noteId = useId();
  const [other, setOther] = useState("");
  const [sending, setSending] = useState(false);
  const send = (clear = false): Promise<unknown> => {
    if (pick === null || !canSend(pick, other, answer.rows) || sending) return Promise.resolve();
    setSending(true);
    const params = { key: view.key, question: answer.question, pick, other, clear };
    return (request("panel.answer", params) as Promise<PanelSent>)
      .then((sent) => {
        askUnsent(sent, () => send(true));
      })
      .catch(ignore)
      .finally(() => {
        setSending(false);
      });
  };
  // A card's option opens the list with it picked (sending stays one more click or ↵, ATT-12); its Send answer
  // sends the picked (recommended) option.
  const onCard = useEffectEvent((card: CardAnswer) => {
    if (card.question !== answer.question) return;
    if (card.kind === "send_answer") {
      void send();
    } else if (answer.rows.some((r) => r.number === card.number)) {
      setPick(card.number);
      options.current?.focus();
    }
  });
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the card's click arrives from outside React, once
    if (fromCard !== null) onCard(fromCard);
  }, [fromCard]);
  return (
    <>
      <div className="panel-answer">
        <div className="panel-answer-question">
          <Icon name="message-circle-question-mark" size={14} />
          <span>{answer.text}</span>
        </div>
        <div
          ref={options}
          className="panel-answer-options"
          role="radiogroup"
          aria-label={answer.text}
          tabIndex={-1}
          onKeyDown={(e) => {
            const n = pickByKey(e.key, answer.rows);
            if (n !== null && !(e.target instanceof HTMLInputElement)) {
              e.preventDefault();
              setPick(n);
            } else if (e.key === "Enter" && !(e.target instanceof HTMLInputElement)) {
              e.preventDefault();
              void send();
            }
          }}
        >
          {answer.rows.map((row) => (
            <button
              key={row.number}
              type="button"
              role="radio"
              aria-checked={pick === row.number}
              aria-describedby={row.note === null ? undefined : `${noteId}-${String(row.number)}`}
              className="panel-answer-option"
              data-picked={pick === row.number || undefined}
              data-other={row.other || undefined}
              onClick={() => {
                setPick(row.number);
              }}
            >
              <span className="panel-answer-radio" />
              <span className="panel-answer-label">
                {row.label}
                {/* The option's note (ATT-17), whole and wrapped: its description. */}
                {row.note !== null && (
                  <span id={`${noteId}-${String(row.number)}`} className="panel-answer-note" aria-hidden>
                    {row.note}
                  </span>
                )}
              </span>
              {row.recommended && (
                <span className="panel-answer-rec">
                  <Icon name="star" size={10} />
                  {answer.recommended_label}
                </span>
              )}
              <span className="panel-answer-number">{row.number}</span>
            </button>
          ))}
          {picksOther(pick, answer.rows) && (
            <input
              {...PLAIN_TEXT}
              className="field-input panel-answer-other"
              value={other}
              placeholder={answer.other_placeholder}
              aria-label={answer.other_placeholder}
              // The field appears because Other… was just picked: typing goes straight into it.
              // eslint-disable-next-line jsx-a11y/no-autofocus -- it opens on the user's own pick
              autoFocus
              onChange={(e) => {
                setOther(e.target.value);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void send();
                }
              }}
            />
          )}
        </div>
      </div>
      <div className="panel-bar">
        <span className="panel-answer-hint">{answer.hint}</span>
        <Button
          label={answer.in_terminal}
          variant="secondary"
          icon="square-terminal"
          onClick={() => {
            // An epic's question is its lead's: its terminal is in the Lead tab.
            showTab(view.epic ? "lead" : "agent");
            requestTerminalFocus(view.key);
          }}
        />
        <Button
          label={answer.send}
          icon="send"
          disabled={!canSend(pick, other, answer.rows)}
          loading={sending}
          onClick={() => {
            void send();
          }}
        />
        {more}
      </div>
    </>
  );
}

/** *SHOP-32 · Validation failed 3 times*: the note goes with the last findings (`panel.answer_agent`). */
function AnswerAgentBox({
  view,
  box,
  onClose,
}: {
  view: PanelView;
  box: PanelAnswerAgent;
  onClose: () => void;
}) {
  const [note, setNote] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const send = () => {
    if (sending || note.trim() === "") return;
    setSending(true);
    setError(null);
    request("panel.answer_agent", { key: view.key, note }).then(onClose, (e: unknown) => {
      setError(asRequestError(e).message);
      setSending(false);
    });
  };
  return (
    <div className="panel-answer-agent">
      <span className="panel-answer-agent-heading">{box.heading}</span>
      <TextArea id="panel-answer-agent-note" value={note} placeholder={box.placeholder} onChange={setNote} />
      <p className="panel-answer-agent-footer">{box.footer}</p>
      {error !== null && <p className="panel-dialog-error">{error}</p>}
      <div className="panel-answer-agent-buttons">
        <Button variant="ghost" icon="check" label={box.cancel} onClick={onClose} />
        <Button icon="send" label={box.send} disabled={note.trim() === ""} loading={sending} onClick={send} />
      </div>
    </div>
  );
}

function Notice({ view, notice }: { view: PanelView; notice: PanelNotice }) {
  return (
    <div className="panel-notice" data-closed={notice.parent_closed || undefined}>
      <Icon name={notice.parent_closed ? "circle-slash" : "hourglass"} size={13} />
      <div className="panel-notice-body">
        <p>{notice.text}</p>
        {notice.choices.length > 0 && (
          <div className="panel-notice-choices">
            {notice.choices.map((choice) => (
              <span key={choice.parent} className="panel-notice-pair">
                <Button
                  compact
                  variant="secondary"
                  icon="rotate-ccw"
                  label={choice.reopen}
                  onClick={() => {
                    request("panel.reopen_parent", { key: view.key, parent: choice.parent }).catch(ignore);
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
                        request("panel.remove_dependency", { key: view.key, parent: choice.parent }),
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

/** A refused model above the bar (AGT-05, *05 → Start failed on the model*, *No result: the validator's model*): what
 * couldn't run (red for Start failed) and the tool's message in quotes. */
function Refusal({ refusal }: { refusal: PanelRefusal }) {
  return (
    <div className="panel-refusal" data-error={refusal.error || undefined}>
      <span className="panel-refusal-heading">{refusal.heading}</span>
      <span className="panel-refusal-message">{refusal.message}</span>
    </div>
  );
}

function StartProblem({ view, problem }: { view: PanelView; problem: PanelStartProblem }) {
  const [log, setLog] = useState<string | null>(null);
  const [shown, setShown] = useState(false);
  return (
    <div className="panel-start-problem">
      <div className="panel-start-callout">
        <div className="panel-start-heading">
          <Icon name="circle-alert" size={14} />
          <span>{problem.heading}</span>
        </div>
        <p className="panel-start-message">{problem.message}</p>
        {problem.has_log && (
          <button
            type="button"
            className="panel-link"
            onClick={() => {
              if (!shown && log === null) {
                (request("panel.setup_log", { key: view.key }) as Promise<string | null>).then(
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
      {shown && log !== null && <pre className="panel-setup-log">{log}</pre>}
      <div className="panel-start-does">
        <span className="panel-start-does-heading">{problem.retry_heading}</span>
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
 * unavailable", "Codex isn't available · hidden in Settings", "Claude Code limit reached · resets 3:45 pm", with Use
 * fallback, Open agent…, Sign in… and Show in Agent tools.
 */
function Holds({ view }: { view: PanelView }) {
  if (view.holds.length === 0) return null;
  return (
    <div className="panel-holds">
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

/**
 * The ticket's choices and its path under the action bar, in every panel state (AGM-09; *05 → Row · Stage choices and
 * path (update AG §9)*): a pick goes to the service at once (`panel.set_choice`), and the panel follows the ticket's
 * change. None for an epic; only the path on a flow without options.
 */
function ChoicesAndPath({ view }: { view: PanelView }) {
  if (view.choices === null && view.path === null) return null;
  return (
    <div className="panel-agents">
      {view.choices !== null && (
        <StageChoices
          key={view.key}
          view={view.choices}
          look="panel"
          onPick={(stage, choice) => {
            request("panel.set_choice", { key: view.key, stage, choice } satisfies PanelChoiceParams).catch(
              ignore,
            );
          }}
        />
      )}
      {view.path !== null && <TicketPath view={view.path} />}
    </div>
  );
}

/** The last signal, the tokens by agent and the Run's pill (the old header's meta row), when there are any. */
function MetaLine({ view }: { view: PanelView }) {
  if (view.signal === null && view.token_split === null && view.run === null) return null;
  return (
    <div className="panel-meta">
      {view.signal && (
        <span className="panel-meta-item" data-stale={view.signal.stale || undefined}>
          <Icon name="activity" size={11} />
          <span>{view.signal.text}</span>
        </span>
      )}
      {view.token_split !== null && (
        <span className="panel-split">
          <Icon name="coins" size={11} />
          <span>{view.token_split}</span>
        </span>
      )}
      {view.run && <RunPill ticket={view.key} pill={view.run} />}
    </div>
  );
}

/**
 * The top of the sheet's Overview (SHT-03's "what it needs from you", T-0465): the old panel header's lower part (the
 * conflict strip, the question with its options or the action bar with its callouts, the Auto-merge stop message,
 * Answer the agent's box), the choices and the path, the queued notice, the open PR's box and the Start interrupted
 * callout. The registered sections (Epic, Stack, Messages) are Overview's own, under the description.
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
  view: PanelView;
  actions: PanelActions;
  /** Shown right above the action bar: the review's manual checks (d0125). */
  aboveBar?: ReactNode;
}) {
  const app = useRegistries();
  const { press, busy, answering, setAnswering, cardAnswer } = actions;
  const UnderTabs = app.panelSections.get(PANEL_UNDER_TABS)?.Component;
  return (
    <div className="panel-overview">
      <div className="panel-header">
        <MetaLine view={view} />
        {view.conflict && (
          <div className="panel-conflict">
            <Icon name="triangle-alert" size={13} />
            <span className="panel-conflict-line">{view.conflict.line}</span>
          </div>
        )}
        {view.answer?.asker != null ? (
          // An epic's lead asks (SHT-07, `s25-11`): its options are buttons.
          <LeadAsks key={view.answer.question} view={view} answer={view.answer} />
        ) : view.answer ? (
          <AnswerBox
            key={view.answer.question}
            view={view}
            answer={view.answer}
            more={null}
            fromCard={cardAnswer}
          />
        ) : (
          <>
            {view.refusal && <Refusal refusal={view.refusal} />}
            <Holds view={view} />
            {aboveBar}
            {view.buttons.length > 0 && (
              <div className="panel-bar">
                {view.buttons.map((button) => (
                  <span key={button.action} className="panel-action-group">
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
        {view.build_aside !== null && <BuildAside line={view.build_aside} />}
        {view.stop_message !== null && (
          <p className="panel-stop-message">
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
      <ChoicesAndPath view={view} />
      {view.notice && <Notice view={view} notice={view.notice} />}
      {UnderTabs && <UnderTabs ticket={view.key} />}
      {view.start_problem && <StartProblem key={view.key} view={view} problem={view.start_problem} />}
    </div>
  );
}

/** "● :4020 ↗ ▢" while the Run is up (*13 → Card & panel-header controls*): Open (with an Open URL) and Stop. */
function RunPill({ ticket, pill }: { ticket: string; pill: PanelRunPill }) {
  return (
    <span className="panel-run">
      <span className="panel-run-dot" />
      <button
        type="button"
        className="panel-run-port"
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
            className="panel-run-icon"
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
          className="panel-run-icon"
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
