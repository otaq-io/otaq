// The Add project form (PRJ-01…PRJ-03; *14 → 3 · Add project · valid*, *Not a git repo*, *Already added*, *Adding a
// second project later*): the folder picker (the system's, through the door), the service's check of the folder,
// the detected name and base branch (another local or remote branch can be picked before adding), and the ticket key
// with its live check and "SHOP-1, SHOP-2, … Can’t be changed later." The page holds what was picked and typed; Rust
// words the rest (`onboarding.form`) and adds the project (`onboarding.add`). The onboarding step and the dialog share
// it. Under the folder, which tools will run their hooks and settings there (d0086); when a tool's trust couldn't be
// recorded, the project is added all the same and the form says which and why, waiting for Continue (PRJ-07, d0085).
// The dialog of a project added after the first has a light Flow choice: Otaq default, Same as <project>, Suggest one.
import { type ReactNode, useEffect, useRef, useState } from "react";

import { Button, Radio, Select, TextInput } from "../../components";
import type { AddProjectFlow } from "../../generated/AddProjectFlow";
import type { AddProjectFlowPick } from "../../generated/AddProjectFlowPick";
import type { AddProjectOutcome } from "../../generated/AddProjectOutcome";
import type { AddProjectParams } from "../../generated/AddProjectParams";
import type { AddProjectView } from "../../generated/AddProjectView";
import { asRequestError } from "../../service";
import { pickFolder as systemPicker, request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import {
  adoptKey,
  type Ask,
  baseLabel,
  flowPicked,
  keyText,
  newForm,
  pickFlow,
  pickFolder,
  sameFlowPick,
  typeBase,
  typeKey,
} from "./logic";

/** How long typing settles before the form is asked again. */
const TYPING_SETTLE_MS = 120;

export interface AddProjectState {
  form: AddProjectParams;
  view: AddProjectView | undefined;
  adding: boolean;
  /** A failure of the request itself. */
  failure: string | null;
  setForm: (change: (form: AddProjectParams) => AddProjectParams) => void;
  choose: () => void;
  add: () => void;
}

/** The form's state: asks Rust for the form on every change, and adds the project when asked. `folder` is read once
 * (a folder dropped later remounts the form). */
export function useAddProject(
  firstRun: boolean,
  folder: string | null,
  onAdded: (key: string, flow: AddProjectFlowPick) => void,
  ask: Ask = request,
): AddProjectState {
  const [form, setFormState] = useState<AddProjectParams>(() => newForm(firstRun, folder));
  const [view, setView] = useState<AddProjectView>();
  const [adding, setAdding] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  /** Added, but a tool's trust wasn't recorded: the key, until Continue. */
  const [added, setAdded] = useState<string | null>(null);
  const latest = useRef(form);

  useEffect(() => {
    latest.current = form;
    let live = true;
    const timer = setTimeout(() => {
      ask("onboarding.form", form).then(
        (answer) => {
          if (!live) return;
          const next = answer as AddProjectView;
          setView(next);
          setFailure(null);
          setFormState((current) => adoptKey(current, form, next));
        },
        (error: unknown) => {
          if (live) setFailure(asRequestError(error).message);
        },
      );
    }, TYPING_SETTLE_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [form, ask]);

  const add = () => {
    if (added !== null) {
      onAdded(added, flowPicked(latest.current));
      return;
    }
    if (!view?.can_add || adding) return;
    setAdding(true);
    ask("onboarding.add", latest.current).then(
      (answer) => {
        setAdding(false);
        const outcome = answer as AddProjectOutcome;
        switch (outcome.kind) {
          case "added":
            onAdded(outcome.key, flowPicked(latest.current));
            break;
          case "added_untrusted":
            setView(outcome.form);
            setAdded(outcome.key);
            break;
          case "refused":
            setView(outcome.form);
            break;
        }
      },
      (error: unknown) => {
        setAdding(false);
        setFailure(asRequestError(error).message);
      },
    );
  };

  const choose = () => {
    if (added !== null) return;
    systemPicker(view?.picker_title ?? "").then(
      (picked) => {
        if (picked !== null) setFormState((f) => pickFolder(f, picked));
      },
      (error: unknown) => {
        setFailure(asRequestError(error).message);
      },
    );
  };

  // Once added, the form only waits for Continue.
  const setForm = added === null ? setFormState : () => undefined;
  return { form, view, adding, failure, setForm, choose, add };
}

/** A form that only draws `view` (the gallery, the tests): nothing is asked or added. */
export function staticAddProject(view: AddProjectView): AddProjectState {
  return {
    form: newForm(view.first_run, view.repo_path),
    view,
    adding: false,
    failure: null,
    setForm: () => undefined,
    choose: () => undefined,
    add: () => undefined,
  };
}

/** The folder row: the path, its check's outline, and Choose folder… / Change…. */
function FolderRow({ view, onChoose }: { view: AddProjectView; onChoose: () => void }) {
  return (
    <div className="add-project-folder" data-look={view.folder_look}>
      <Icon name="folder-git-2" size={18} />
      <span className="add-project-path" data-empty={view.folder_look === "empty" || undefined}>
        {view.folder}
      </span>
      <Button
        label={view.choose}
        variant={view.choose_quiet ? "ghost" : "secondary"}
        {...(view.choose_quiet ? {} : { icon: "folder-open" as const })}
        onClick={onChoose}
      />
    </div>
  );
}

/** The dialog's light Flow choice: Otaq default (selected), Same as <project>, Suggest one for <new project>. */
function FlowChoice({
  flow,
  picked,
  onPick,
}: {
  flow: AddProjectFlow;
  picked: AddProjectFlowPick;
  onPick: (pick: AddProjectFlowPick) => void;
}) {
  return (
    <fieldset className="add-project-flow">
      <legend className="add-project-key-title">{flow.label}</legend>
      {flow.options.map((option) => (
        <div key={option.name} className="add-project-flow-option">
          <Radio
            name="add-project-flow"
            label={option.name}
            checked={sameFlowPick(option.pick, picked)}
            onChange={(on) => {
              if (on) onPick(option.pick);
            }}
          />
          <span className="add-project-flow-line">{option.line}</span>
        </div>
      ))}
      <span className="add-project-detected-note">{flow.foot}</span>
    </fieldset>
  );
}

/** The fields under the folder: the error, the detected values, the key. */
export function AddProjectFields({ state }: { state: AddProjectState }) {
  const { view, form, setForm, add } = state;
  if (view === undefined) return null;
  const detected: ReactNode[] = [];
  if (view.name)
    detected.push(
      <div key="name" className="add-project-detected-row">
        <Icon name={view.name.icon} size={13} />
        <span className="add-project-detected-label">{view.name.label}</span>
        <span className="add-project-detected-value">{view.name.value}</span>
      </div>,
    );
  if (view.base)
    detected.push(
      <div key="base" className="add-project-detected-row">
        <Icon name={view.base.icon} size={13} />
        <span className="add-project-detected-label">{view.base.label}</span>
        <span className="add-project-base">
          <Select
            label={view.base.label}
            value={baseLabel(form, view)}
            icon={null}
            mono
            trailing="chevrons-up-down"
            items={view.base.items}
            search={view.base.search}
            onChange={(id) => {
              setForm((f) => typeBase(f, id));
            }}
          />
        </span>
        <span className="add-project-detected-note">{view.base.note}</span>
      </div>,
    );
  const keyField = view.key && (
    <div className="add-project-key">
      <span className="add-project-key-label">
        <span className="add-project-key-title">{view.key.label}</span>
        <span className="add-project-key-note">{view.key.note}</span>
      </span>
      <span className="add-project-key-input">
        <TextInput
          value={keyText(form, view)}
          onChange={(v) => {
            setForm((f) => typeKey(f, v));
          }}
          placeholder={view.key.placeholder}
          label={view.key.label}
          invalid={view.key.error !== null}
          mono
        />
      </span>
      {view.key.error !== null ? (
        <span className="add-project-key-error">{view.key.error}</span>
      ) : (
        view.key.hint !== null && (
          <span className="add-project-key-hint">
            <Icon name="info" size={12} />
            <span>{view.key.hint}</span>
          </span>
        )
      )}
    </div>
  );
  const detectedBox = detected.length > 0 && (
    <div className="add-project-detected" data-boxed={view.first_run || undefined}>
      {detected}
    </div>
  );
  return (
    // ↵ in a field adds the project, as the form's button.
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- only reads ↵ from the text fields inside
    <div
      className="add-project-fields"
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.nativeEvent.isComposing && e.target instanceof HTMLInputElement) {
          e.preventDefault();
          add();
        }
      }}
    >
      <FolderRow view={view} onChoose={state.choose} />
      {view.trust_line !== null && (
        <span className="add-project-trust">
          <Icon name="shield" size={13} />
          <span>{view.trust_line}</span>
        </span>
      )}
      {view.trust_error !== null && (
        <span className="add-project-failure" role="alert">
          {view.trust_error}
        </span>
      )}
      {view.error && (
        <div className="add-project-error" role="alert">
          <Icon name="circle-x" size={13} />
          <span className="add-project-error-text">
            <span className="add-project-error-title">{view.error.title}</span>
            <span>{view.error.detail}</span>
          </span>
        </div>
      )}
      {/* The onboarding card shows what was detected, then the key; the dialog leads with the key. */}
      {view.first_run ? detectedBox : keyField}
      {view.first_run ? keyField : detectedBox}
      {view.flow && (
        <FlowChoice
          flow={view.flow}
          picked={flowPicked(form)}
          onPick={(pick) => {
            setForm((f) => pickFlow(f, pick));
          }}
        />
      )}
      {view.add_error !== null && <span className="add-project-failure">{view.add_error}</span>}
      {state.failure !== null && <span className="add-project-failure">{state.failure}</span>}
    </div>
  );
}

/** The Add button, with its in-progress label; Continue once the project was added without a tool's trust. */
export function AddButton({ state }: { state: AddProjectState }) {
  const { view, adding } = state;
  if (view === undefined) return null;
  if (view.trust_error !== null) return <Button label={view.continue_label} onClick={state.add} />;
  return (
    <Button
      label={adding ? view.adding : view.add}
      icon="plus"
      disabled={!view.can_add || adding}
      onClick={state.add}
    />
  );
}
