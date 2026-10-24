// Form pieces (*01 · Components → Badges & Inputs → Fields*): a labelled field with a hint or an error under it, the
// text input and text area (the webview's own editing, selection and IME), the check box and the radio button.
// Which error shows when comes from Rust (`fields.rs`: once touched, or from the service as it sent it).
import { type ReactNode, type Ref, useId } from "react";
import { PLAIN_TEXT, PROSE_TEXT } from "./plainText";

export interface FieldProps {
  label?: string;
  /** A muted line under the control. */
  hint?: string | null | undefined;
  /** A red line under the control, in place of the hint. */
  error?: string | null | undefined;
  /** The control, given the id its label points at. */
  children: (id: string) => ReactNode;
}

/** A label (11/500), the control, then the error (red) or the hint (muted). */
export function Field({ label, hint, error, children }: FieldProps) {
  const id = useId();
  const note = error ?? hint;
  return (
    <div className="field">
      {label !== undefined && (
        <label className="field-label" htmlFor={id}>
          {label}
        </label>
      )}
      {children(id)}
      {note !== null && note !== undefined && (
        <span className="field-note" data-error={(error !== null && error !== undefined) || undefined}>
          {note}
        </span>
      )}
    </div>
  );
}

interface TextProps {
  id?: string;
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  placeholder?: string;
  /** Code-like values (file names, branches) in mono 12. */
  mono?: boolean;
  /** Draws the red border while the field has an error. */
  invalid?: boolean;
  disabled?: boolean;
  /** Names it for screen readers when no Field label does. */
  label?: string;
  ref?: Ref<HTMLInputElement>;
}

/** A 30 px text input; focus shows the 1.5 px focus border. */
export function TextInput({
  id,
  value,
  onChange,
  onBlur,
  placeholder,
  mono,
  invalid,
  disabled,
  label,
  ref,
}: TextProps) {
  return (
    <input
      {...PLAIN_TEXT}
      ref={ref}
      id={id}
      className="text-input"
      type="text"
      value={value}
      placeholder={placeholder}
      disabled={disabled}
      aria-label={label}
      aria-invalid={invalid || undefined}
      data-mono={mono || undefined}
      onChange={(e) => {
        onChange(e.target.value);
      }}
      onBlur={onBlur}
    />
  );
}

interface TextAreaProps extends Omit<TextProps, "ref"> {
  rows?: number;
  ref?: Ref<HTMLTextAreaElement>;
}

/** Multi-line text (What and why, a note). */
export function TextArea({
  id,
  value,
  onChange,
  onBlur,
  placeholder,
  mono,
  invalid,
  disabled,
  label,
  rows = 3,
  ref,
}: TextAreaProps) {
  return (
    <textarea
      {...PROSE_TEXT}
      ref={ref}
      id={id}
      className="text-input text-area"
      value={value}
      rows={rows}
      placeholder={placeholder}
      disabled={disabled}
      aria-label={label}
      aria-invalid={invalid || undefined}
      data-mono={mono || undefined}
      onChange={(e) => {
        onChange(e.target.value);
      }}
      onBlur={onBlur}
    />
  );
}

interface ChoiceProps {
  checked: boolean;
  label: ReactNode;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

/** A check box with its label; the label's click toggles it too. */
export function Checkbox({ checked, label, disabled, onChange }: ChoiceProps) {
  return (
    <label className="choice" data-disabled={disabled || undefined}>
      <input
        type="checkbox"
        className="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => {
          onChange(e.target.checked);
        }}
      />
      <span>{label}</span>
    </label>
  );
}

/** One radio button of a group (`name`); arrows move within the group, as the webview does. */
export function Radio({ name, checked, label, disabled, onChange }: ChoiceProps & { name: string }) {
  return (
    <label className="choice" data-disabled={disabled || undefined}>
      <input
        type="radio"
        className="radio"
        name={name}
        checked={checked}
        disabled={disabled}
        onChange={(e) => {
          onChange(e.target.checked);
        }}
      />
      <span>{label}</span>
    </label>
  );
}
