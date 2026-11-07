// Settings → Keyboard shortcuts (T-0395; KEY-05, KEY-04, KEY-02; d0097, d0089; *15 · Keyboard shortcuts*,
// `s15-46`…`s15-55`): the single-letter switch, then every action's row with each key its own cell. Click a cell (or
// focus it and press ↵) and it records: the next key is sent as pressed (`keyboard.record`) and Rust answers with
// the map's text and what the checks say; a lone `g` makes the cell wait for its letter, Esc cancels, Remove key
// takes the key away. A key another action has asks in place (Use here / Cancel), a refused one says why and the
// cell keeps recording. "Works inside terminals" warns first (Cancel / Let it through). Reset all in the header asks
// first. Every word and every check is `keyboard_page.rs`'s / `keymap_edit.rs`'s; this keeps only the cell that
// records and the answer shown under its row.
//
// While a cell records it carries `data-key-recorder`, so the app's key dispatcher leaves every key to it
// (`KEY_RECORDER`, `inKeyRecorder` in `app/keys.ts`, agreed with T-0391), and it stops each key there so Settings' Esc and the webview's own keys
// don't act on it.
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { isModifierKey } from "../../app/keys";
import { Button, Checkbox, ConfirmDialog, IconButton, Kbd, Toggle } from "../../components";
import type { KeyEdit } from "../../generated/KeyEdit";
import type { KeyEditAnswer } from "../../generated/KeyEditAnswer";
import type { ShortcutsCell } from "../../generated/ShortcutsCell";
import type { ShortcutsKey } from "../../generated/ShortcutsKey";
import type { ShortcutsMoment } from "../../generated/ShortcutsMoment";
import type { ShortcutsPage } from "../../generated/ShortcutsPage";
import type { ShortcutsPress } from "../../generated/ShortcutsPress";
import type { ShortcutsRecordAnswer } from "../../generated/ShortcutsRecordAnswer";
import type { ShortcutsRow } from "../../generated/ShortcutsRow";
import type { ShortcutsSlot } from "../../generated/ShortcutsSlot";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { useSettings } from "./context";
import "./keyboard.css";

/** The attribute of a cell while it records: the dispatcher leaves every key to it (`KEY_RECORDER` in keys.ts). */
const RECORDING = "data-key-recorder";

/** The page's calls (`api/keyboard.rs`); the gallery passes its own. */
export interface ShortcutsCalls {
  record: (slot: ShortcutsSlot, press: ShortcutsPress, afterG: boolean) => Promise<ShortcutsRecordAnswer>;
  edit: (edit: KeyEdit) => Promise<KeyEditAnswer>;
  resetRow: (keys: readonly ShortcutsSlot[], confirmed: boolean) => Promise<KeyEditAnswer>;
}

export const serviceCalls: ShortcutsCalls = {
  record: (slot, press, afterG) =>
    request("keyboard.record", { ...slot, ...press, after_g: afterG }) as Promise<ShortcutsRecordAnswer>,
  edit: (edit) => request("keyboard.edit", edit) as Promise<KeyEditAnswer>,
  resetRow: (keys, confirmed) => request("keyboard.reset_row", { keys, confirmed }) as Promise<KeyEditAnswer>,
};

/** What "Use here" or "Let it through" sends again. */
type Retry =
  | { kind: "record"; slot: ShortcutsSlot; key: string }
  | { kind: "reset"; keys: readonly ShortcutsSlot[] }
  | { kind: "terminals"; slot: ShortcutsSlot; on: boolean; letThrough: boolean };

const sameSlot = (a: ShortcutsSlot | null | undefined, b: ShortcutsSlot) =>
  a != null && a.slot === b.slot && JSON.stringify(a.action) === JSON.stringify(b.action);

/** The key event as Rust reads it. */
export function pressOf(e: KeyboardEvent): ShortcutsPress {
  return {
    key: e.key,
    code: e.code,
    modifiers: { control: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, platform: e.metaKey, function: false },
  };
}

/** Chips of a key: one Kbd, or `g` then its letter. */
function Chips({ chips, then }: { chips: readonly string[]; then: string }) {
  return (
    <>
      {chips.map((chip, i) => (
        <span key={chip + String(i)} className="shortcut-chip-group">
          {i > 0 && <span className="shortcut-then">{then}</span>}
          <Kbd keys={chip} />
        </span>
      ))}
    </>
  );
}

interface CellProps {
  k: ShortcutsKey;
  page: ShortcutsPage;
  recording: boolean;
  /** Takes the focus when it starts recording (a click or ↵; not a gallery frame drawn recording). */
  focus: boolean;
  afterG: boolean;
  /** A key waiting for "Use here", drawn in this cell. */
  waiting: string | null;
  onStart: () => void;
  onPress: (press: ShortcutsPress) => void;
  onCancel: () => void;
}

/** One key's cell: its chips, "no key", or while it records, "Press the new keys…". */
function KeyCell({ k, page, recording, focus, afterG, waiting, onStart, onPress, onCancel }: CellProps) {
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const element = button.current;
    if (recording && focus) element?.focus();
  }, [recording, focus]);

  useEffect(() => {
    const element = button.current;
    if (!recording || !element) return;
    // On the cell itself, so it runs before Settings' Esc (on the window's box) and stops the key there.
    const onKey = (e: KeyboardEvent) => {
      if (isModifierKey(e) || e.isComposing) return;
      const plain = !e.ctrlKey && !e.altKey && !e.metaKey;
      // Tab moves on (to Remove key, or away, which cancels), so the cell never traps the keyboard.
      if (e.key === "Tab" && plain) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && plain && !e.shiftKey) onCancel();
      else onPress(pressOf(e));
    };
    element.addEventListener("keydown", onKey);
    return () => {
      element.removeEventListener("keydown", onKey);
    };
  }, [recording, onPress, onCancel]);

  if (recording) {
    return (
      <button
        ref={button}
        type="button"
        className="shortcut-cell"
        data-recording
        {...{ [RECORDING]: "" }}
        onBlur={(e) => {
          if (!e.relatedTarget?.closest("[data-remove-key]")) onCancel();
        }}
      >
        <Icon name="keyboard" size={12} />
        <span>{afterG ? page.texts.after_g : page.texts.recording}</span>
      </button>
    );
  }
  const chips = waiting === null ? k.chips : waiting.startsWith("g ") ? ["g", waiting.slice(2)] : [waiting];
  if (chips.length === 0) {
    return (
      <button type="button" className="shortcut-cell" data-no-key onClick={onStart}>
        {page.texts.no_key}
      </button>
    );
  }
  return (
    <button
      type="button"
      className="shortcut-cell"
      data-changed={k.changed || waiting !== null || undefined}
      data-dimmed={k.dimmed || undefined}
      onClick={onStart}
    >
      <Chips chips={chips} then={page.texts.then} />
    </button>
  );
}

/** What a row shows under it: the question, the reason or the warning, with their buttons. */
function Answer({
  answer,
  page,
  onYes,
  onCancel,
}: {
  answer: KeyEditAnswer;
  page: ShortcutsPage;
  onYes: () => void;
  onCancel: () => void;
}) {
  if (answer.outcome === "done") return null;
  const [tone, icon, text] =
    answer.outcome === "taken"
      ? (["ask", "circle-question-mark", answer.question] as const)
      : answer.outcome === "refused"
        ? (["refused", "ban", answer.reason] as const)
        : (["warn", "triangle-alert", answer.warning] as const);
  return (
    <div className="shortcut-answer" data-tone={tone} role={tone === "refused" ? "alert" : "group"}>
      <Icon name={icon} size={14} />
      <span className="shortcut-answer-text">{text}</span>
      {tone !== "refused" && (
        <>
          <Button variant="ghost" label={page.texts.cancel} onClick={onCancel} />
          <Button label={tone === "ask" ? page.texts.use_here : page.texts.let_through} onClick={onYes} />
        </>
      )}
    </div>
  );
}

/** Reset all and its question (d0097: it asks first). */
function ResetAll({ page, onReset }: { page: ShortcutsPage; onReset: () => void }) {
  const [open, setOpen] = useState(false);
  const r = page.reset_all;
  return (
    <>
      <Button
        variant="secondary"
        icon="rotate-ccw"
        label={r.label}
        disabled={!r.enabled}
        onClick={() => {
          setOpen(true);
        }}
      />
      {open && (
        <ConfirmDialog
          title={r.question}
          body={r.body}
          confirmLabel={r.label}
          cancelLabel={r.cancel}
          confirmIcon="rotate-ccw"
          width={480}
          onConfirm={() => {
            setOpen(false);
            onReset();
          }}
          onCancel={() => {
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

export interface ShortcutsViewProps {
  page: ShortcutsPage;
  calls: ShortcutsCalls;
  /** The moment to start in (the gallery's frames); none on the page. */
  moment?: ShortcutsMoment;
  /** Only these rows, by id, without the switch (the gallery's frames); all when absent. */
  rows?: readonly string[];
  /** Where Reset all goes: Settings' header (a portal), else drawn above the page with its title. `null`: not yet. */
  headerSlot?: HTMLElement | null;
  /** Draws the header (title and Reset all) when there is no slot. */
  header?: boolean;
  /** An edit failed (the request, not its checks). */
  onError?: (message: string) => void;
  /** Opens on the short list (Settings): the shortcuts that matter most, with All shortcuts to see every group. */
  short?: boolean;
}

/** The page: header, switch and rows, and the moment (the recording cell, the answer under a row). */
export function ShortcutsView({
  page,
  calls,
  moment,
  rows,
  headerSlot,
  header = true,
  onError,
  short = false,
}: ShortcutsViewProps) {
  const [all, setAll] = useState(!short);
  const [recording, setRecording] = useState<ShortcutsSlot | null>(moment?.recording ?? null);
  const [afterG, setAfterG] = useState(moment?.after_g ?? false);
  const [answer, setAnswer] = useState(moment?.answer ?? null);
  const [retry, setRetry] = useState<Retry | null>(null);
  const busy = useRef(false);
  // A cell recording because it was clicked takes the focus; one drawn recording (the gallery's frames) doesn't.
  const [started, setStarted] = useState(false);

  const failed = (e: unknown) => {
    onError?.(asRequestError(e).message);
  };

  /** Shows what an edit of `row` answered: done clears everything; the rest waits under the row. */
  const settle = (
    row: string,
    slot: ShortcutsSlot,
    a: KeyEditAnswer,
    again: Retry | null,
    key: string | null,
  ) => {
    if (a.outcome === "done") {
      setRecording(null);
      setAnswer(null);
      setRetry(null);
      return;
    }
    // A refusal while recording keeps the cell recording; the question and the warning stop it.
    if (a.outcome !== "refused" || !sameSlot(recording, slot)) setRecording(null);
    setAfterG(false);
    setAnswer({ row, slot, key, answer: a });
    setRetry(a.outcome === "refused" ? null : again);
  };

  const start = (slot: ShortcutsSlot) => {
    setStarted(true);
    setRecording(slot);
    setAfterG(false);
    setAnswer(null);
    setRetry(null);
  };
  const cancel = () => {
    setRecording(null);
    setAfterG(false);
    setAnswer((now) => (now?.answer.outcome === "refused" ? null : now));
  };

  const pressed = (row: string, slot: ShortcutsSlot, press: ShortcutsPress) => {
    if (busy.current) return;
    busy.current = true;
    calls
      .record(slot, press, afterG)
      .then((a) => {
        if (a.outcome === "then_letter") {
          setAfterG(true);
          setAnswer(null);
        } else if (a.outcome === "edited") {
          settle(
            row,
            slot,
            a.answer,
            { kind: "record", slot, key: a.key },
            a.answer.outcome === "taken" ? a.key : null,
          );
        }
      }, failed)
      .finally(() => {
        busy.current = false;
      });
  };

  const send = (row: string, slot: ShortcutsSlot, edit: KeyEdit, again: Retry | null) => {
    calls.edit(edit).then((a) => {
      settle(row, slot, a, again, null);
    }, failed);
  };

  const yes = () => {
    if (!answer || !retry) return;
    const { row, slot } = answer;
    switch (retry.kind) {
      case "record":
        send(
          row,
          slot,
          { edit: "record", action: slot.action, slot: slot.slot, key: retry.key, confirmed: true },
          null,
        );
        return;
      case "reset":
        calls.resetRow(retry.keys, true).then((a) => {
          settle(row, slot, a, null, null);
        }, failed);
        return;
      case "terminals": {
        // "Let it through" answers the warning; "Use here" (its Shift form taken) confirms the move too.
        const confirmed = answer.answer.outcome === "taken";
        const letThrough = retry.letThrough || answer.answer.outcome === "warn";
        const again: Retry = { ...retry, letThrough };
        send(
          row,
          slot,
          {
            edit: "works_in_terminals",
            action: slot.action,
            slot: slot.slot,
            on: retry.on,
            let_through: letThrough,
            confirmed,
          },
          again,
        );
        return;
      }
    }
  };

  const shownRows = page.groups.flatMap((g) => g.rows);
  const only = rows === undefined ? null : rows.map((id) => shownRows.find((r) => r.id === id));

  const rowView = (row: ShortcutsRow) => {
    const mine = answer?.row === row.id ? answer : null;
    const recordingHere = recording !== null && row.cells.some((c) => cellHas(c, recording));
    const recordingKey =
      recording !== null
        ? row.cells
            .flatMap((c) => (c.kind === "keys" ? c.halves : []))
            .find((h) => sameSlot(recording, h.slot))
        : undefined;
    const removable = recordingKey !== undefined && recordingKey.chips.length > 0;
    const keyCell = (k: ShortcutsKey) => (
      <KeyCell
        key={JSON.stringify(k.slot)}
        k={k}
        page={page}
        recording={sameSlot(recording, k.slot)}
        focus={started}
        afterG={afterG}
        waiting={mine?.answer.outcome === "taken" && sameSlot(mine.slot, k.slot) ? mine.key : null}
        onStart={() => {
          start(k.slot);
        }}
        onPress={(p) => {
          pressed(row.id, k.slot, p);
        }}
        onCancel={cancel}
      />
    );
    const cell = (c: ShortcutsCell, i: number) => (
      <span key={i} className="shortcut-cell-slot">
        {i > 0 && <span className="shortcut-or">{page.texts.or}</span>}
        {c.kind === "range" ? (
          <span
            className="shortcut-cell"
            data-range
            data-changed={c.changed || undefined}
            data-dimmed={c.dimmed || undefined}
          >
            <Kbd keys={c.text} />
          </span>
        ) : c.halves.length === 1 && c.halves[0] ? (
          keyCell(c.halves[0])
        ) : (
          <span className="shortcut-pair">
            {c.halves.map((h, j) => (
              <span key={j} className="shortcut-pair-half">
                {j > 0 && <span className="shortcut-or">/</span>}
                {keyCell(h)}
              </span>
            ))}
          </span>
        )}
      </span>
    );
    const check = row.terminals;
    return (
      <div key={row.id} className="shortcut-row-block" data-row={row.id}>
        <div
          className="shortcut-row"
          data-recording={recordingHere || undefined}
          data-dimmed={row.dimmed || undefined}
        >
          <div className="shortcut-name">
            <span>{row.title}</span>
            {(row.changed || mine?.answer.outcome === "taken") && (
              <span className="shortcut-default">{row.default_line}</span>
            )}
          </div>
          <div className="shortcut-keys">{row.cells.map(cell)}</div>
          {removable && recording !== null && (
            <button
              type="button"
              className="button"
              data-variant="ghost"
              data-remove-key
              onMouseDown={(e) => {
                e.preventDefault();
              }}
              onClick={() => {
                send(
                  row.id,
                  recording,
                  { edit: "remove", action: recording.action, slot: recording.slot },
                  null,
                );
              }}
            >
              <Icon name="x" size={14} />
              <span>{page.texts.remove_key}</span>
            </button>
          )}
          {check && (
            <div className="shortcut-terminals">
              <Checkbox
                checked={check.on}
                disabled={check.fixed}
                label={page.texts.works_in_terminals}
                onChange={(on) => {
                  send(
                    row.id,
                    check.slot,
                    {
                      edit: "works_in_terminals",
                      action: check.slot.action,
                      slot: check.slot.slot,
                      on,
                      let_through: false,
                      confirmed: false,
                    },
                    { kind: "terminals", slot: check.slot, on, letThrough: false },
                  );
                }}
              />
              {check.in_terminals !== null && (
                <span className="shortcut-in-terminals">{check.in_terminals}</span>
              )}
            </div>
          )}
          <div className="shortcut-reset">
            {row.reset !== null && (
              <IconButton
                icon="rotate-ccw"
                label={page.texts.reset}
                onClick={() => {
                  const keys = row.reset ?? [];
                  const first = keys[0];
                  if (!first) return;
                  calls.resetRow(keys, false).then((a) => {
                    settle(row.id, first, a, { kind: "reset", keys }, null);
                  }, failed);
                }}
              />
            )}
          </div>
        </div>
        {recordingHere && (
          <p className="shortcut-hint">
            {removable ? page.texts.recording_hint : page.texts.recording_hint_no_key}
          </p>
        )}
        {mine && (
          <Answer
            answer={mine.answer}
            page={page}
            onYes={yes}
            onCancel={() => {
              setAnswer(null);
              setRetry(null);
            }}
          />
        )}
      </div>
    );
  };

  const resetAll = (
    <ResetAll
      page={page}
      onReset={() => {
        calls.edit({ edit: "reset_all" }).then(() => {
          setAnswer(null);
          setRecording(null);
        }, failed);
      }}
    />
  );

  return (
    <section
      className="shortcut-settings"
      aria-label={page.title}
      data-terminals-column={page.terminals_column || undefined}
    >
      {headerSlot ? createPortal(resetAll, headerSlot) : null}
      {headerSlot === undefined && header && (
        <header className="shortcut-settings-header">
          <h2>{page.title}</h2>
          {resetAll}
        </header>
      )}
      {only === null && !all ? (
        <>
          <p className="settings-intro">{page.short.intro}</p>
          <div className="shortcut-settings-group">
            {page.short.rows.map((id) => {
              const row = shownRows.find((r) => r.id === id);
              return row && rowView(row);
            })}
          </div>
          <span className="shortcut-settings-more">
            <Button
              variant="secondary"
              compact
              label={page.short.all_label}
              onClick={() => {
                setAll(true);
              }}
            />
          </span>
        </>
      ) : only === null ? (
        <>
          <p className="settings-intro">{page.intro}</p>
          <div className="shortcut-settings-letters">
            <div className="shortcut-settings-letters-text">
              <span className="shortcut-settings-letters-label">{page.letters.label}</span>
              <span className="shortcut-settings-letters-description">{page.letters.description}</span>
            </div>
            <Toggle
              on={page.letters.on}
              label={page.letters.label}
              showLabel={false}
              onChange={(on) => {
                calls.edit({ edit: "letters", on }).catch(failed);
              }}
            />
          </div>
          {page.terminals_note !== null && <TerminalsNote text={page.terminals_note} />}
          {page.groups.map((g) => (
            <div key={g.title} className="shortcut-settings-group">
              <h3 className="shortcut-settings-group-title">{g.title}</h3>
              {g.rows.map(rowView)}
            </div>
          ))}
          {short && (
            <span className="shortcut-settings-more">
              <Button
                variant="secondary"
                compact
                label={page.short.fewer_label}
                onClick={() => {
                  setAll(false);
                }}
              />
            </span>
          )}
        </>
      ) : (
        <div className="shortcut-settings-group">
          {page.terminals_note !== null && <TerminalsNote text={page.terminals_note} />}
          {only.map((r) => r && rowView(r))}
        </div>
      )}
    </section>
  );
}

function TerminalsNote({ text }: { text: string }) {
  return (
    <p className="shortcut-settings-terminals-note">
      <Icon name="info" size={13} />
      <span>{text}</span>
    </p>
  );
}

function cellHas(cell: ShortcutsCell, slot: ShortcutsSlot): boolean {
  return cell.kind === "keys" && cell.halves.some((h) => sameSlot(slot, h.slot));
}

const loadPage = () => request("keyboard.view") as Promise<ShortcutsPage>;

/** The registered section: the page from `keyboard.view`, again on every settings change. */
export function KeyboardSection() {
  const { headerSlot } = useSettings();
  const { view, error } = useServiceView(loadPage, ["settings_changed"]);
  const [failure, setFailure] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <>
      {failure !== null && (
        <p className="settings-error" role="alert">
          {failure}
        </p>
      )}
      <ShortcutsView
        page={view}
        calls={serviceCalls}
        headerSlot={headerSlot ?? null}
        onError={setFailure}
        short
      />
    </>
  );
}
