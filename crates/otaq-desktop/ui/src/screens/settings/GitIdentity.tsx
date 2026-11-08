// The name and email git signs commits with, when git has none (`GitIdentityForm`): two fields and where Save writes
// them. The Tools step's git row and Settings → Git & pull requests show it with its own Save; the merge dialog shows
// the fields and saves them with its "Save and merge". Save is `tools.git_identity`, which writes them into the
// user's own git settings only on that click; a refusal ("Type your email first.") shows under the fields.
import { useState } from "react";

import { Button, Field, TextInput } from "../../components";
import type { GitIdentityForm } from "../../generated/GitIdentityForm";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import "./git-identity.css";

export interface GitIdentity {
  name: string;
  setName: (name: string) => void;
  email: string;
  setEmail: (email: string) => void;
  saving: boolean;
  error: string | null;
  /** Saves them; `true` once git has them. */
  save: () => Promise<boolean>;
}

/** The form's state, starting from what git has. */
export function useGitIdentity(form: GitIdentityForm, ask: typeof request = request): GitIdentity {
  const [name, setName] = useState(form.name);
  const [email, setEmail] = useState(form.email);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await ask("tools.git_identity", { name, email });
      return true;
    } catch (e: unknown) {
      setError(asRequestError(e).message);
      return false;
    } finally {
      setSaving(false);
    }
  };
  return { name, setName, email, setEmail, saving, error, save };
}

/** The two fields, the refusal or where they're saved. */
export function GitIdentityFields({ form, state }: { form: GitIdentityForm; state: GitIdentity }) {
  return (
    <div className="git-identity-fields">
      <Field label={form.name_label}>
        {(id) => (
          <TextInput
            id={id}
            value={state.name}
            placeholder={form.name_placeholder}
            onChange={state.setName}
            disabled={state.saving}
          />
        )}
      </Field>
      <Field label={form.email_label}>
        {(id) => (
          <TextInput
            id={id}
            value={state.email}
            placeholder={form.email_placeholder}
            onChange={state.setEmail}
            disabled={state.saving}
          />
        )}
      </Field>
      {state.error !== null ? (
        <p className="git-identity-note" data-error role="alert">
          {state.error}
        </p>
      ) : (
        <p className="git-identity-note">{form.note}</p>
      )}
    </div>
  );
}

/** Under git's row: the title, why, the fields and Save; `onSaved` checks the tools again. */
export function GitIdentityBox({ form, onSaved }: { form: GitIdentityForm; onSaved: () => void }) {
  const state = useGitIdentity(form);
  return (
    <div className="git-identity">
      <p className="git-identity-title">{form.title}</p>
      <p className="git-identity-text">{form.text}</p>
      <GitIdentityFields form={form} state={state} />
      <div className="git-identity-foot">
        <Button
          label={form.save}
          compact
          loading={state.saving}
          onClick={() => {
            void state.save().then((saved) => {
              if (saved) onSaved();
            });
          }}
        />
      </div>
    </div>
  );
}
