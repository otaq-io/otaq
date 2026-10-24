// The Environment variables row (T-0300; ENV-01, ENV-03, d0082; *15 → Row · Environment variables (0082)* and *… ·
// edit, remove, errors (0082)*): the row's text and the always-visible hint, then a full-width list of NAME = value
// lines, Add variable and the "applies after" line. Every word and every problem comes from the Rust row
// (`SettingsControl` "env_vars"); this keeps only what is being typed. A field saves on ↵, blur or Esc (T-0506: Esc
// once saves and stays in Settings, Esc again closes it). A new line is kept here until focus leaves it, ↵ or Esc, which
// save it once anything is typed (a value with no name too: the Rust row then marks it); Esc drops only an empty one.
import { type RefObject, useEffect, useRef, useState } from "react";

import { Tooltip, PLAIN_TEXT } from "../../components";
import type { SettingsControl } from "../../generated/SettingsControl";
import type { SettingsEnvVar } from "../../generated/SettingsEnvVar";
import type { SettingsRow } from "../../generated/SettingsRow";
import { Icon } from "../../theme/Icon";
import { fieldId, useSettings } from "./context";

type EnvVarsControl = Extract<SettingsControl, { kind: "env_vars" }>;

/** Esc inside `ref` goes to `take` first; when it takes it, Settings doesn't close (its own Esc listener is on the
 * window's box, a native listener that React's handlers would reach too late). */
function useEscape(ref: RefObject<HTMLElement | null>, take: () => boolean) {
  const latest = useRef(take);
  useEffect(() => {
    latest.current = take;
  });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && latest.current()) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    el.addEventListener("keydown", onKey);
    return () => {
      el.removeEventListener("keydown", onKey);
    };
  }, [ref]);
}

/** One saved field: saves on ↵, blur and Esc when changed; Esc on an unchanged field closes Settings. */
function VarField({
  field,
  value,
  placeholder,
  label,
  invalid,
}: {
  field: string;
  value: string;
  placeholder: string;
  label: string;
  invalid: boolean;
}) {
  const { edit } = useSettings();
  const input = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState(value);
  const [base, setBase] = useState(value);
  // What was last sent, until the service's answer comes back: a second Esc then closes Settings.
  const sent = useRef<string | null>(null);
  // A fresh value from the service (a save, another window) replaces the draft.
  if (base !== value) {
    setBase(value);
    setDraft(value);
  }
  useEffect(() => {
    sent.current = null;
  }, [value]);
  const unsaved = () => draft !== value && draft !== sent.current;
  const save = () => {
    if (!unsaved()) return;
    sent.current = draft;
    edit(field, draft);
  };
  useEscape(input, () => {
    if (!unsaved()) return false;
    save();
    return true;
  });
  return (
    <input
      {...PLAIN_TEXT}
      ref={input}
      id={fieldId(field)}
      className="text-input settings-envvar-input"
      type="text"
      data-mono
      autoComplete="off"
      value={draft}
      placeholder={placeholder}
      aria-label={label}
      aria-invalid={invalid || undefined}
      onChange={(e) => {
        setDraft(e.target.value);
      }}
      onBlur={save}
      onKeyDown={(e) => {
        if (e.key !== "Enter") return;
        e.preventDefault();
        save();
      }}
    />
  );
}

function Remove({ label, onRemove }: { label: string; onRemove: () => void }) {
  return (
    <Tooltip text={label}>
      <button type="button" className="settings-envvar-remove" aria-label={label} onClick={onRemove}>
        <Icon name="x" size={12} />
      </button>
    </Tooltip>
  );
}

function Problem({ text }: { text: string }) {
  return (
    <p className="settings-envvar-problem">
      <Icon name="circle-alert" size={12} />
      <span>{text}</span>
    </p>
  );
}

function SavedVar({ index, row, control }: { index: number; row: SettingsEnvVar; control: EnvVarsControl }) {
  const { edit } = useSettings();
  const at = `env_vars.${String(index)}`;
  return (
    <>
      <div className="settings-envvar">
        <span className="settings-envvar-name">
          <VarField
            field={`${at}.name`}
            value={row.name}
            placeholder={control.name_placeholder}
            label={control.name_placeholder}
            invalid={row.problem !== null}
          />
        </span>
        <span className="settings-envvar-eq" aria-hidden>
          =
        </span>
        <VarField
          field={`${at}.value`}
          value={row.value}
          placeholder={control.value_placeholder}
          label={row.name === "" ? control.value_placeholder : row.name}
          invalid={false}
        />
        <Remove
          label={row.remove_label}
          onRemove={() => {
            edit("env_vars.remove", index);
          }}
        />
      </div>
      {row.problem !== null && <Problem text={row.problem} />}
    </>
  );
}

/** The line Add variable opens: NAME focused; ↵, Esc or leaving it saves it once anything is typed (a value with no
 * name is saved too, and marked); × drops it, and Esc drops it while it is empty.
 * `onDone(refocus)`: after ↵, Esc or × the focus goes back to Add variable, since the line goes with it. */
function NewVar({ control, onDone }: { control: EnvVarsControl; onDone: (refocus: boolean) => void }) {
  const { edit } = useSettings();
  const line = useRef<HTMLDivElement>(null);
  const name = useRef<HTMLInputElement>(null);
  // Ends once: moving the focus away after ↵ blurs the line again.
  const done = useRef(false);
  const [draft, setDraft] = useState({ name: "", value: "" });
  useEffect(() => {
    name.current?.focus();
  }, []);
  const finish = (save: boolean, refocus: boolean) => {
    if (done.current) return;
    done.current = true;
    if (save && (draft.name !== "" || draft.value !== "")) edit("env_vars.add", draft);
    onDone(refocus);
  };
  useEscape(line, () => {
    finish(true, true);
    return true;
  });
  const field = (part: "name" | "value", placeholder: string) => (
    <input
      {...PLAIN_TEXT}
      ref={part === "name" ? name : undefined}
      className="text-input settings-envvar-input"
      type="text"
      data-mono
      autoComplete="off"
      value={draft[part]}
      placeholder={placeholder}
      aria-label={placeholder}
      onChange={(e) => {
        setDraft({ ...draft, [part]: e.target.value });
      }}
    />
  );
  return (
    <div
      ref={line}
      className="settings-envvar"
      role="presentation"
      onKeyDown={(e) => {
        if (e.key !== "Enter" || (e.target as Element).tagName !== "INPUT") return;
        e.preventDefault();
        finish(true, true);
      }}
      onBlur={(e) => {
        if (line.current?.contains(e.relatedTarget)) return;
        finish(true, false);
      }}
    >
      <span className="settings-envvar-name">{field("name", control.name_placeholder)}</span>
      <span className="settings-envvar-eq" aria-hidden>
        =
      </span>
      {field("value", control.value_placeholder)}
      <Remove
        label={control.drop_label}
        onRemove={() => {
          finish(false, true);
        }}
      />
    </div>
  );
}

export function EnvVars({ control }: { control: EnvVarsControl }) {
  const [adding, setAdding] = useState(false);
  const add = useRef<HTMLButtonElement>(null);
  return (
    <div className="settings-envvars">
      {control.rows.length === 0 && !adding && <div className="settings-envvars-empty">{control.empty}</div>}
      {control.rows.map((row, i) => (
        <SavedVar key={i} index={i} row={row} control={control} />
      ))}
      {adding && (
        <NewVar
          control={control}
          onDone={(refocus) => {
            setAdding(false);
            if (refocus) add.current?.focus();
          }}
        />
      )}
      <button
        ref={add}
        type="button"
        className="settings-envvars-add"
        onClick={() => {
          setAdding(true);
        }}
      >
        <Icon name="plus" size={12} />
        <span>{control.add_label}</span>
      </button>
      <p className="settings-envvars-applies">{control.applies}</p>
    </div>
  );
}

/** The whole row: label, description and the hint across the width, then the list under them. */
export function EnvVarsRow({ row }: { row: SettingsRow }) {
  const control = row.control;
  if (control.kind !== "env_vars") return null;
  return (
    <div className="settings-row" data-stacked>
      <div className="settings-row-text">
        <span className="settings-row-label">{row.label}</span>
        <span className="settings-row-description">{row.description}</span>
        <span className="settings-envvars-hint">
          <code>{control.hint_example}</code>
          <span>{control.hint}</span>
        </span>
      </div>
      <EnvVars control={control} />
    </div>
  );
}
