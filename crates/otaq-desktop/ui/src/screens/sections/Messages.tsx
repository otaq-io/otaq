// The Messages section (MSG-01, MSG-03, MSG-06; d0030; *05 → Messages · agent-to-agent (SHOP-40.2), Message
// states, Dark · Messages*): each question between this ticket and its linked tickets with its answer and status
// line, oldest first. A key opens that ticket; a question sent to the user (Needs you) is answered here. The epic lead's threads
// (T-0290, *05 → Row · Messages · the lead (0083)*) name the lead at their other end ("SHOP-40.2 → Wishlist lead").
import { useCallback, useState } from "react";

import type { TicketProps } from "../../app/registry";
import { openTicket } from "../../app/selection";
import { Button, PLAIN_TEXT } from "../../components";
import type { MessageLineView } from "../../generated/MessageLineView";
import type { MessagesView } from "../../generated/MessagesView";
import type { MessageThreadView } from "../../generated/MessageThreadView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { SECTION_EVENTS } from "./logic";

const ignore = () => undefined;

function End({ ticket, name, you }: { ticket: string | null; name: string | null; you: string }) {
  if (name !== null) return <span className="message-name">{name}</span>;
  if (ticket === null) return <span className="message-you">{you}</span>;
  return (
    <button
      type="button"
      className="message-key"
      onClick={() => {
        openTicket(ticket);
      }}
    >
      {ticket}
    </button>
  );
}

function LineRow({ line, you }: { line: MessageLineView; you: string }) {
  return (
    <div className="message-line">
      <div className="message-route">
        <Icon name={line.from === null && line.from_name === null ? "user" : "bot"} size={12} />
        <End ticket={line.from} name={line.from_name} you={you} />
        <Icon name="arrow-right" size={11} />
        <End ticket={line.to} name={line.to_name} you={you} />
        <span className="message-time">{line.time}</span>
      </div>
      <p className="message-text">{line.text}</p>
      <div className="message-status" data-tone={line.tone}>
        <Icon name={line.icon} size={11} />
        <span>{line.status}</span>
      </div>
    </div>
  );
}

function Reply({ view, thread }: { view: MessagesView; thread: MessageThreadView }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const send = () => {
    if (sending || text.trim() === "") return;
    setSending(true);
    request("sections.answer_message", { id: thread.id, answer: text })
      .then(() => {
        setOpen(false);
        setText("");
      }, ignore)
      .finally(() => {
        setSending(false);
      });
  };
  if (!open) {
    return (
      <div>
        <Button
          compact
          label={view.answer}
          icon="message-circle-reply"
          onClick={() => {
            setOpen(true);
          }}
        />
      </div>
    );
  }
  return (
    <div className="message-reply">
      <input
        {...PLAIN_TEXT}
        className="text-input"
        value={text}
        placeholder={view.placeholder}
        aria-label={view.placeholder}
        // The box opens on the user's Answer click: typing goes straight into it.
        // eslint-disable-next-line jsx-a11y/no-autofocus -- it opens on the user's own click
        autoFocus
        onChange={(e) => {
          setText(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            send();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            setOpen(false);
          }
        }}
      />
      <Button
        compact
        label={view.send}
        icon="send"
        loading={sending}
        disabled={text.trim() === ""}
        onClick={send}
      />
    </div>
  );
}

export function MessagesBody({ view }: { view: MessagesView }) {
  return (
    <section className="section section-messages" aria-label={view.title}>
      <div className="section-head">
        <Icon name="messages-square" size={12} />
        <span className="section-title">{view.title}</span>
        <span className="section-spacer" />
        <span className="section-count">{view.heading}</span>
      </div>
      <div className="section-messages-threads">
        {view.threads.map((thread) => (
          <div key={thread.id} className="message-thread">
            <LineRow line={thread.question} you={view.you} />
            {thread.answer && (
              <div className="message-answer">
                <LineRow line={thread.answer} you={view.you} />
              </div>
            )}
            {thread.answerable && <Reply view={view} thread={thread} />}
          </div>
        ))}
      </div>
    </section>
  );
}

export function MessagesSection({ ticket }: TicketProps) {
  const load = useCallback(
    () => request("sections.messages", { key: ticket }) as Promise<MessagesView | null>,
    [ticket],
  );
  const { view } = useServiceView(load, SECTION_EVENTS);
  return view ? <MessagesBody view={view} /> : null;
}
