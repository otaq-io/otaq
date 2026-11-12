// A ticket's answer in the making: the picked or ticked rows and Something else's text of its open question. Overview
// unmounts when another sheet tab shows, so the draft lives here, outside the tab body (as the description's draft
// does), keyed by the ticket and the question's id: back on Overview the box shows what was picked, ticked and typed. A
// ticket keeps one draft, its open question's; a new question (a replaced one has a new id) starts from its own
// recommended options, the sent answer and a closed question drop it.
import { useEffect } from "react";

import { createStore, useStore } from "../../app/store";
import type { SheetAnswer } from "../../generated/SheetAnswer";
import { initialPicks, pressPick } from "../sheet/actionRoutes";

export interface AnswerDraft {
  /** The question's id: a draft is only ever its own question's. */
  question: string;
  picks: number[];
  other: string;
}

/** Every ticket's draft, by ticket key. */
export const answerDrafts = createStore<ReadonlyMap<string, AnswerDraft>>(new Map());

/** `ticket`'s draft for `answer`'s question; a fresh one (the recommended rows, no text) when it has none. */
export function draftOf(
  drafts: ReadonlyMap<string, AnswerDraft>,
  ticket: string,
  answer: SheetAnswer,
): AnswerDraft {
  const kept = drafts.get(ticket);
  if (kept?.question === answer.question) return kept;
  return { question: answer.question, picks: initialPicks(answer.rows), other: "" };
}

function changeDraft(ticket: string, answer: SheetAnswer, change: (now: AnswerDraft) => AnswerDraft): void {
  answerDrafts.update((drafts) => {
    const next = new Map(drafts);
    next.set(ticket, change(draftOf(drafts, ticket, answer)));
    return next;
  });
}

/** Drops `ticket`'s draft unless it is `question`'s (`null`: the ticket asks nothing now). */
export function forgetAnswerDraft(ticket: string, question: string | null): void {
  const kept = answerDrafts.get().get(ticket);
  if (kept === undefined || kept.question === question) return;
  answerDrafts.update((drafts) => {
    const next = new Map(drafts);
    next.delete(ticket);
    return next;
  });
}

/** Drops `ticket`'s draft if it is `question`'s. */
function dropAnswerDraft(ticket: string, question: string): void {
  if (answerDrafts.get().get(ticket)?.question !== question) return;
  forgetAnswerDraft(ticket, null);
}

/** Keeps `ticket`'s drafts to the question its view shows: a closed or replaced question's draft goes. */
export function useForgetClosedAnswer(ticket: string, question: string | null): void {
  useEffect(() => {
    forgetAnswerDraft(ticket, question);
  }, [ticket, question]);
}

/** A ticket's answer box's draft, and its ways to change it (as `useAnswerPicks`, with Something else's text). */
export function useAnswerDraft(
  ticket: string,
  answer: SheetAnswer,
): {
  picks: number[];
  other: string;
  /** A click or number key: picks the row, or ticks / unticks it on a question that takes several answers. */
  press: (n: number) => void;
  /** A card's option: the row is picked (ticked, never unticked). */
  pickRow: (n: number) => void;
  setOther: (other: string) => void;
  /** The answer went: the draft goes with it. */
  sent: () => void;
} {
  const draft = draftOf(useStore(answerDrafts), ticket, answer);
  const multiple = answer.multiple === true;
  return {
    picks: draft.picks,
    other: draft.other,
    press: (n) => {
      changeDraft(ticket, answer, (now) => ({ ...now, picks: pressPick(now.picks, n, multiple) }));
    },
    pickRow: (n) => {
      changeDraft(ticket, answer, (now) =>
        now.picks.includes(n) ? now : { ...now, picks: pressPick(now.picks, n, multiple) },
      );
    },
    setOther: (other) => {
      changeDraft(ticket, answer, (now) => ({ ...now, other }));
    },
    sent: () => {
      dropAnswerDraft(ticket, answer.question);
    },
  };
}
