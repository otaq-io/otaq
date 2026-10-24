// The Ask bar (T-0458; SHB-03, d0113, d0068, d0087; *24 → Row · The Project agent* `s24-05`, *Row · The two views*
// `s24-03`): the `ask_bar` slot, floating at the bottom centre of the main area over the board, the List and the
// ticket, hidden while the Project agent panel is open. Typing shows "ASK ABOUT" above it (↑ / ↓ choose, Return sends
// and opens the panel, Esc clears or leaves); a reply the user hasn't seen shows "New reply" and the conversation's
// name, which a click opens. Everything it says is `ask_bar.view`'s; it also hands that view to the rail's button.
import { useCallback, useEffect, useRef, useState } from "react";

import { shellLayout } from "../../app/layout";
import { useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { Kbd, PLAIN_TEXT } from "../../components";
import type { AskBarParams } from "../../generated/AskBarParams";
import type { AskBarView } from "../../generated/AskBarView";
import type { AskSuggestion } from "../../generated/AskSuggestion";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { chosenAgent, shownAgents } from "../pane/logic";
import { askBar, lookedCount, nextChoice, openOn, sendFromBar } from "./logic";

/** The service events that change what the bar says or suggests. */
export const ASK_BAR_EVENTS = [
  "project_agent_changed",
  "ticket_changed",
  "ticket_deleted",
  "projects_changed",
  // The user's keys on it (KEY-08).
  "settings_changed",
] as const;

const ignore = () => undefined;

export interface AskBarBodyProps {
  view: AskBarView;
  text: string;
  /** The field has the keyboard: the suggestions show while it holds words. */
  typing: boolean;
  /** The suggestion Return sends with. */
  choice: number;
  onText?: (text: string) => void;
  onTyping?: (typing: boolean) => void;
  onChoice?: (choice: number) => void;
  onSend?: (suggestion: AskSuggestion) => void;
  onReply?: (agent: string) => void;
}

/** The bar for a loaded view (also what the gallery draws). */
export function AskBarBody({
  view,
  text,
  typing,
  choice,
  onText,
  onTyping,
  onChoice,
  onSend,
  onReply,
}: AskBarBodyProps) {
  const field = useRef<HTMLInputElement>(null);
  const words = text.trim() !== "";
  const suggesting = typing && words;
  const chosen = view.suggestions[Math.min(choice, view.suggestions.length - 1)];
  const reply = words ? null : view.reply;
  return (
    <div className="ask" data-typing={suggesting || undefined}>
      {suggesting && (
        <div className="ask-suggestions">
          <span className="ask-suggestions-title">{view.title}</span>
          {view.suggestions.map((s) => (
            <button
              key={`${s.about}-${s.ticket ?? s.agent ?? ""}`}
              type="button"
              tabIndex={-1}
              className="ask-suggestion"
              data-chosen={s === chosen || undefined}
              // The field keeps the keyboard: a click sends with this row.
              onMouseDown={(e) => {
                e.preventDefault();
              }}
              onClick={() => {
                onSend?.(s);
              }}
            >
              <Icon name={s.icon} size={13} />
              <span className="ask-suggestion-label">{s.label}</span>
            </button>
          ))}
        </div>
      )}
      {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions --
          a click anywhere on the bar puts the cursor in its field, which has the keys */}
      <div
        className="ask-bar"
        data-focused={typing || undefined}
        onClick={() => {
          field.current?.focus();
        }}
      >
        <span className="ask-bot">
          <Icon name={view.icon} size={16} />
        </span>
        <input
          {...PLAIN_TEXT}
          ref={field}
          className="ask-input"
          aria-label={view.placeholder}
          placeholder={reply ? view.short_placeholder : view.placeholder}
          value={text}
          onChange={(e) => {
            onText?.(e.target.value);
          }}
          onFocus={() => {
            onTyping?.(true);
          }}
          onBlur={() => {
            onTyping?.(false);
          }}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return;
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              if (!suggesting) return;
              e.preventDefault();
              onChoice?.(nextChoice(choice, view.suggestions.length, e.key === "ArrowDown" ? 1 : -1));
            } else if (e.key === "Enter") {
              e.preventDefault();
              if (chosen && words) onSend?.(chosen);
            } else if (e.key === "Escape") {
              e.preventDefault();
              if (words) onText?.("");
              else e.currentTarget.blur();
            }
          }}
        />
        {reply && (
          <button
            type="button"
            className="ask-reply"
            onClick={(e) => {
              e.stopPropagation();
              onReply?.(reply.agent);
            }}
          >
            <span>{reply.label}</span>
            <span className="ask-reply-name">{reply.name}</span>
            <span className="ask-dot" />
          </button>
        )}
        {words && <Kbd keys={view.send_keys} />}
        {view.keys !== null && <Kbd keys={view.keys} />}
      </div>
    </div>
  );
}

/** One project's bar: follows the service, the selected ticket and the panel. */
function ProjectAskBar({ project, ticket }: { project: string; ticket: string | null }) {
  const open = useStore(shellLayout).agentOpen;
  const shown = chosenAgent(useStore(shownAgents), project);
  const looked = useStore(lookedCount);
  const load = useCallback(() => {
    const params: AskBarParams = {
      project,
      open,
      ...(ticket !== null && { ticket }),
      ...(shown !== null && { shown }),
    };
    return request("ask_bar.view", params) as Promise<AskBarView>;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new look is a new load: the reply just seen goes
  }, [project, ticket, open, shown, looked]);
  const { view } = useServiceView(load, ASK_BAR_EVENTS);
  useEffect(() => {
    askBar.set(view);
  }, [view]);
  useEffect(
    () => () => {
      askBar.set(undefined);
    },
    [],
  );
  const [text, setText] = useState("");
  const [typing, setTyping] = useState(false);
  const [choice, setChoice] = useState(0);
  if (!view || open) return null;
  return (
    <AskBarBody
      view={view}
      text={text}
      typing={typing}
      choice={choice}
      onText={(next) => {
        setText(next);
        setChoice(0);
      }}
      onTyping={setTyping}
      onChoice={setChoice}
      onSend={(suggestion) => {
        sendFromBar(project, text, suggestion).then((sent) => {
          if (sent) setText("");
        }, ignore);
      }}
      onReply={(agent) => {
        openOn(project, agent);
      }}
    />
  );
}

/** The `ask_bar` slot: the shown project's bar (nothing before a project is added). */
export function AskBar() {
  const { project, ticket } = useSelection();
  if (project === null) return null;
  return <ProjectAskBar key={project} project={project} ticket={ticket} />;
}
