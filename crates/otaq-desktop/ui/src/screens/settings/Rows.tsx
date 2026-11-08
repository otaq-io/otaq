// A Settings section and its rows (*15 · Settings*): label and description on the left (320 px), the control on the
// right, a line under each row. Every text and every control's state comes from the Rust row (`SettingsRow`); the
// controls keep only what is being typed. Text saves on blur and on ↵ (a multi-line field: blur and ⌘↵ / Ctrl+↵).
import { type KeyboardEvent, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import {
  AnchoredMenu,
  Button,
  chooseThemeMode,
  Checkbox,
  CodeText,
  IconButton,
  pickTheme,
  Segmented,
  SetByLine,
  TextArea,
  TextInput,
  ThemePicker,
  Toggle,
  Tooltip,
  useThemePicker,
} from "../../components";
import type { MenuItem } from "../../generated/MenuItem";
import type { SettingsControl } from "../../generated/SettingsControl";
import type { SettingsProvider } from "../../generated/SettingsProvider";
import type { SettingsRow } from "../../generated/SettingsRow";
import type { SettingsRowButton } from "../../generated/SettingsRowButton";
import type { SettingsSection } from "../../generated/SettingsSection";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { fieldId, sectionOf, useSettings } from "./context";
import { WarmOffDialog } from "./Dialogs";
import { EnvVarsRow } from "./EnvVars";
import { Dropdown } from "./ModelPicker";

type Of<K extends SettingsControl["kind"]> = Extract<SettingsControl, { kind: K }>;

/** A text field that saves when it loses focus or on ↵, only when it changed. */
function TextControl({ field, control }: { field: string; control: Of<"text"> }) {
  const { edit } = useSettings();
  const [draft, setDraft] = useState(control.value);
  const [base, setBase] = useState(control.value);
  // A fresh value from the service (another window, a reload) replaces the draft.
  if (base !== control.value) {
    setBase(control.value);
    setDraft(control.value);
  }
  const save = () => {
    if (draft !== control.value) edit(field, draft);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const submit = control.multiline ? e.key === "Enter" && (e.metaKey || e.ctrlKey) : e.key === "Enter";
    if (submit) {
      e.preventDefault();
      save();
    }
  };
  const common = {
    id: fieldId(field),
    value: draft,
    onChange: setDraft,
    onBlur: save,
    placeholder: control.placeholder,
    mono: control.mono,
    invalid: control.problem !== null,
  };
  return (
    <div className="settings-field" onKeyDown={onKeyDown} role="presentation">
      {control.multiline ? <TextArea {...common} rows={3} /> : <TextInput {...common} />}
      {control.problem !== null && <span className="settings-problem">{control.problem}</span>}
    </div>
  );
}

/** − value + (*15 → Agent limits*, terminal font size); the value is clamped by the Rust edit. */
function Stepper({ field, label, control }: { field: string; label: string; control: Of<"stepper"> }) {
  const { edit } = useSettings();
  const step = (by: number) => {
    edit(field, Math.min(control.max, Math.max(control.min, control.value + by)));
  };
  return (
    <span className="settings-stepper" role="group" aria-label={label}>
      <button
        type="button"
        aria-label="−"
        disabled={control.disabled || control.value <= control.min}
        onClick={() => {
          step(-1);
        }}
      >
        <Icon name="minus" size={13} />
      </button>
      <span className="settings-stepper-value" data-empty={control.empty !== null || undefined}>
        {control.empty ?? (
          <>
            {control.value}
            {control.suffix}
          </>
        )}
      </span>
      <button
        type="button"
        aria-label="+"
        disabled={control.disabled || control.value >= control.max}
        onClick={() => {
          step(1);
        }}
      >
        <Icon name="plus" size={13} />
      </button>
    </span>
  );
}

function EnvFiles({ control }: { control: Of<"env_files"> }) {
  const { edit } = useSettings();
  const [adding, setAdding] = useState("");
  return (
    <div className="settings-env">
      {control.rows.map((row, i) => (
        <div key={`${String(i)}-${row.value}`} className="settings-env-row">
          <div className="settings-env-line">
            <TextControl
              field={`env_files.${String(i)}`}
              control={{
                kind: "text",
                value: row.value,
                placeholder: "",
                mono: true,
                multiline: false,
                problem: row.problem,
              }}
            />
            <IconButton
              icon="x"
              label={control.remove_label}
              onClick={() => {
                edit("env_files.remove", i);
              }}
            />
          </div>
        </div>
      ))}
      <div
        role="presentation"
        onKeyDown={(e) => {
          if (e.key !== "Enter" || adding.trim() === "") return;
          e.preventDefault();
          edit("env_files.add", adding);
          setAdding("");
        }}
      >
        <TextInput
          value={adding}
          onChange={setAdding}
          placeholder={control.add_placeholder}
          label={control.add_placeholder}
          mono
        />
      </div>
    </div>
  );
}

/** The base branch dropdown: the repository's branches, asked for when it opens. */
function Branch({ control }: { control: Of<"branch"> }) {
  const { edit, project } = useSettings();
  const trigger = useRef<HTMLButtonElement>(null);
  const [items, setItems] = useState<readonly MenuItem[] | null>(null);
  const open = () => {
    if (project === null) return;
    void request("settings.branches", { project }).then(
      (list) => {
        setItems(list as MenuItem[]);
      },
      () => {
        setItems(null);
      },
    );
  };
  return (
    <>
      <button
        ref={trigger}
        type="button"
        className="settings-branch"
        aria-haspopup="menu"
        aria-expanded={items !== null}
        onClick={open}
      >
        <Icon name="git-branch" size={13} />
        <span>{control.value}</span>
        <Icon name="chevron-down" size={13} />
      </button>
      {items && (
        <AnchoredMenu
          anchor={trigger}
          label={control.value}
          items={items}
          list
          width={280}
          search={control.search}
          onChoose={(id) => {
            edit("base_branch", id);
          }}
          onClose={(how) => {
            setItems(null);
            if (how === "key") trigger.current?.focus();
          }}
        />
      )}
    </>
  );
}

function Themes() {
  const picker = useThemePicker();
  if (picker.error) return <p className="settings-problem">{picker.error.message}</p>;
  if (!picker.view) return null;
  return (
    <ThemePicker
      view={picker.view}
      inRows
      onPick={(key) => {
        void pickTheme(key);
      }}
      onMode={(mode) => {
        void chooseThemeMode(mode);
      }}
    />
  );
}

/** Where a provider is set up on this computer: app Settings → Git & pull requests. */
export const PROVIDER_SET_UP_SECTION = "git";

/** The PR provider, detected when the page opens (it reads the remote and asks the CLI for its login). */
function useProvider(project: string | null): SettingsProvider | null {
  const [provider, setProvider] = useState<SettingsProvider | null>(null);
  useEffect(() => {
    if (project === null) return;
    let live = true;
    void request("project_settings.provider", { project }).then(
      (p) => {
        if (live) setProvider(p as SettingsProvider);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [project]);
  return provider;
}

/** "Pull requests through: GitHub · otaq-io/shop-web": the repository's provider, a fact of its remote, and how its
 * tool stands on this computer, with Set up… (app Settings) when it isn't ready here. */
function ProviderRow({ row }: { row: SettingsRow }) {
  const { project } = useSettings();
  const provider = useProvider(project);
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <span className="settings-row-label">{row.label}</span>
        {row.description !== "" && <span className="settings-row-description">{row.description}</span>}
      </div>
      <div className="settings-row-control settings-provider-control">
        {provider && (
          <>
            <span className="settings-value settings-provider-line" data-boxed>
              <Icon name="git-pull-request" size={12} />
              <span>{provider.line}</span>
            </span>
            {provider.status !== "" && (
              <span className="settings-provider" data-look={provider.look}>
                <Icon name={provider.icon} size={13} />
                <span>{provider.status}</span>
                {provider.set_up !== null && (
                  <button
                    type="button"
                    className="settings-link"
                    onClick={() => {
                      openOverlay("settings", { section: PROVIDER_SET_UP_SECTION });
                    }}
                  >
                    {provider.set_up}
                  </button>
                )}
              </span>
            )}
          </>
        )}
      </div>
    </div>
  );
}

/** Delivery: the flow's own setting (Merge into main · Pull request · Either; On Approve · Automatically), shown as
 * the flow's; a pick writes a new flow version (`delivery.delivers`, `delivery.mode`). */
function DeliveryControl({ row, control }: { row: SettingsRow; control: Of<"delivery"> }) {
  const { edit } = useSettings();
  return (
    <span className="settings-delivery">
      <span className="settings-delivery-choices">
        <Segmented
          label={row.label}
          options={control.delivers.map((choice) => choice.label)}
          selected={Math.max(
            0,
            control.delivers.findIndex((choice) => choice.id === control.delivers_value),
          )}
          onSelect={(i) => {
            const choice = control.delivers[i];
            if (choice && choice.id !== control.delivers_value) edit(`${row.field}.delivers`, choice.id);
          }}
        />
        {control.modes.length > 0 && (
          <Segmented
            label={`${row.label}: when`}
            options={control.modes.map((choice) => choice.label)}
            selected={Math.max(
              0,
              control.modes.findIndex((choice) => choice.id === control.mode_value),
            )}
            disabled={control.modes_off}
            onSelect={(i) => {
              const choice = control.modes[i];
              if (choice && choice.id !== control.mode_value) edit(`${row.field}.mode`, choice.id);
            }}
          />
        )}
        {control.either !== null && <span className="settings-note">{control.either}</span>}
      </span>
      <span className="settings-delivery-note">
        <Icon name="workflow" size={13} />
        <span>{control.note}</span>
      </span>
    </span>
  );
}

function Control({ row }: { row: SettingsRow }) {
  const { edit, act } = useSettings();
  const c = row.control;
  switch (c.kind) {
    case "text":
      return <TextControl field={row.field} control={c} />;
    case "notifications_switch":
      return null;
    case "checkbox":
      return null;
    case "toggle":
      return (
        <Toggle
          on={c.on}
          label={row.label}
          showLabel={false}
          onChange={(on) => {
            edit(row.field, on);
          }}
        />
      );
    case "stepper":
      return <Stepper field={row.field} label={row.label} control={c} />;
    case "segmented":
      return (
        <Segmented
          label={row.label}
          options={c.choices.map((choice) => choice.label)}
          selected={Math.max(
            0,
            c.choices.findIndex((choice) => choice.id === c.value),
          )}
          onSelect={(i) => {
            const choice = c.choices[i];
            if (choice) edit(row.field, choice.id);
          }}
        />
      );
    case "select":
      return (
        <Dropdown
          id={fieldId(row.field)}
          label={row.label}
          value={
            <span className="select-value">
              <span>{c.choices.find((choice) => choice.id === c.value)?.label ?? c.value}</span>
            </span>
          }
          choices={() =>
            c.choices.map((choice) => ({
              kind: "option" as const,
              id: choice.id,
              label: choice.label,
              detail: null,
              mono: false,
              detailMono: false,
              checked: choice.id === c.value,
              icon: null,
            }))
          }
          readOnly={false}
          className="settings-select"
          onChoose={(id) => {
            if (id !== c.value) edit(row.field, id);
          }}
        />
      );
    case "note":
      return <span className="settings-note">{c.text}</span>;
    case "value":
      return (
        <span className="settings-value" data-mono={c.mono || undefined} data-boxed={c.boxed || undefined}>
          {c.text}
        </span>
      );
    case "path":
      return (
        <span className="settings-path">
          <span className="settings-value" data-mono data-boxed>
            {c.text}
          </span>
          <Button
            variant="secondary"
            icon="folder-open"
            label={c.reveal_label}
            onClick={() => {
              void request("settings.reveal", { target: c.reveal });
            }}
          />
        </span>
      );
    case "button":
      return (
        <span>
          <Button
            variant={c.destructive ? "destructive" : "secondary"}
            icon={c.icon}
            label={c.label}
            onClick={() => {
              act(c.action, row.field);
            }}
          />
        </span>
      );
    case "env_files":
      return <EnvFiles control={c} />;
    case "env_vars":
      return null;
    case "warm_build":
      return null;
    case "branch":
      return <Branch control={c} />;
    case "themes":
      return null;
    case "provider":
      return null;
    case "delivery":
      return <DeliveryControl row={row} control={c} />;
    case "about":
      return null;
  }
}

/** "Use global": empties a project's own value so the global one applies again. */
function Reset({ row }: { row: SettingsRow }) {
  const { edit } = useSettings();
  if (row.reset === null) return null;
  return (
    <button
      type="button"
      className="settings-reset"
      onClick={() => {
        edit(row.field, "");
      }}
    >
      {row.reset}
    </button>
  );
}

function Row({ row }: { row: SettingsRow }) {
  const { act } = useSettings();
  if (row.control.kind === "provider") return <ProviderRow row={row} />;
  if (row.control.kind === "env_vars") return <EnvVarsRow row={row} />;
  if (row.control.kind === "warm_build") return <WarmBuildRow row={row} control={row.control} />;
  if (row.control.kind === "notifications_switch")
    return <NotificationsSwitchRow row={row} control={row.control} />;
  if (row.control.kind === "checkbox") return <CheckboxRow row={row} control={row.control} />;
  // *Theme*: the row's text, then the picker across the whole width.
  if (row.control.kind === "themes")
    return (
      <>
        <div className="settings-row" data-open>
          <div className="settings-row-text">
            <span className="settings-row-label">{row.label}</span>
            <span className="settings-row-description">{row.description}</span>
          </div>
        </div>
        <Themes />
      </>
    );
  return (
    <div className="settings-row" data-dimmed={row.dimmed || undefined}>
      <div className="settings-row-text">
        <span className="settings-row-label">{row.label}</span>
        {row.description !== "" && (
          <span className="settings-row-description">
            <CodeText text={row.description} />
          </span>
        )}
        {row.set_by !== null && (
          <SetByLine
            setBy={row.set_by}
            onUndo={() => {
              act("undo_run_command", row.set_by?.app ?? null);
            }}
          />
        )}
      </div>
      <div className="settings-row-control">
        <Control row={row} />
        <Reset row={row} />
        {row.button && <RowButton button={row.button} />}
      </div>
    </div>
  );
}

/** "Start new worktrees from a warm build" (WRM-05, WRM-06; *15 → Row · Start new worktrees from a warm build (0082)*).
 * Turning it off asks first only when the row carries a confirmation (it would delete a warm build); turning it on
 * never asks. Unavailable: disabled and off, the reason under the text and in the switch's tooltip. */
export function WarmBuildRow({ row, control }: { row: SettingsRow; control: Of<"warm_build"> }) {
  const { edit } = useSettings();
  const [asking, setAsking] = useState(false);
  const toggle = (
    <Toggle
      on={control.on}
      label={row.label}
      showLabel={false}
      disabled={control.disabled}
      onChange={(on) => {
        if (!on && control.confirm !== null) setAsking(true);
        else edit(row.field, on);
      }}
    />
  );
  return (
    <div className="settings-row" data-unavailable={control.disabled || undefined}>
      <div className="settings-row-text">
        <span className="settings-row-label">{row.label}</span>
        <span className="settings-row-description">{row.description}</span>
        {control.reason !== null && (
          <span className="settings-warm-line">
            <Icon name="ban" size={12} />
            <span>{control.reason}</span>
          </span>
        )}
        {control.clearing !== null && (
          <span className="settings-warm-line" data-clearing>
            <Icon name="loader-circle" size={12} />
            <span>{control.clearing}</span>
          </span>
        )}
      </div>
      <div className="settings-row-control">
        {control.reason !== null ? (
          <span className="settings-warm-switch">
            <Tooltip text={control.reason}>{toggle}</Tooltip>
          </span>
        ) : (
          toggle
        )}
      </div>
      {asking && control.confirm !== null && (
        <WarmOffDialog
          confirm={control.confirm}
          onConfirm={() => {
            setAsking(false);
            edit(row.field, false);
          }}
          onCancel={() => {
            setAsking(false);
          }}
        />
      )}
    </div>
  );
}

/** "OS notifications for blocking items" with what macOS allows under the row's text and the one action that fits
 * (ATT-14, d0090): Allow notifications brings up macOS's question, Open System Settings opens Otaq's entry. The line
 * shows whatever the switch's state. */
export function NotificationsSwitchRow({
  row,
  control,
}: {
  row: SettingsRow;
  control: Of<"notifications_switch">;
}) {
  const { edit, act } = useSettings();
  const status = control.status;
  return (
    <div className="settings-row settings-row-switch">
      <div className="settings-row-text">
        <span className="settings-row-label">{row.label}</span>
        <span className="settings-row-description">{row.description}</span>
        {status !== null && (
          <span className="settings-os-status">
            <span>{status.text}</span>
            {status.action !== null && (
              <button
                type="button"
                className="settings-link"
                onClick={() => {
                  if (status.action) act(status.action.action);
                }}
              >
                {status.action.label}
              </button>
            )}
          </span>
        )}
      </div>
      <div className="settings-row-control">
        <Toggle
          on={control.on}
          label={row.label}
          showLabel={false}
          onChange={(on) => {
            edit(row.field, on);
          }}
        />
      </div>
    </div>
  );
}

/** A checkbox row: a notification kind or Play a sound (ATT-16), indented under the main switch; disabled and dimmed,
 * not hidden, while the switch is off. */
export function CheckboxRow({ row, control }: { row: SettingsRow; control: Of<"checkbox"> }) {
  const { edit } = useSettings();
  return (
    <div
      className="settings-row settings-checkbox-row"
      data-indented={control.indented || undefined}
      data-dimmed={row.dimmed || undefined}
    >
      <Checkbox
        checked={control.on}
        disabled={control.disabled}
        label={
          <span className="settings-checkbox-text">
            <span className="settings-row-label">{row.label}</span>
            {row.description !== "" && <span className="settings-row-description">{row.description}</span>}
          </span>
        }
        onChange={(on) => {
          edit(row.field, on);
        }}
      />
    </div>
  );
}

/** Run command's Ask the agent (T-0232, T-0342): the run-command dialog opens and the agent starts on this click (P1). */
function RowButton({ button }: { button: SettingsRowButton }) {
  const { project } = useSettings();
  if (project === null) return null;
  const shown = (
    <Button
      variant="secondary"
      compact
      icon={button.icon}
      label={button.label}
      onClick={() => {
        openOverlay("run_command", { project, ask: true });
      }}
    />
  );
  return button.tooltip === null ? shown : <Tooltip text={button.tooltip}>{shown}</Tooltip>;
}

export function Section({ section }: { section: SettingsSection<string> }) {
  return (
    <section className="settings-section" data-section={section.id} aria-label={section.title ?? section.id}>
      {section.title !== null && <h3 className="settings-section-title">{section.title}</h3>}
      {section.intro !== null && <p className="settings-intro">{section.intro}</p>}
      {section.rows.map((row) => (
        <Row key={`${row.field}-${row.label}`} row={row} />
      ))}
      {section.note !== null && (
        <p className="settings-right-now">
          <Icon name="info" size={13} />
          <span>{section.note}</span>
        </p>
      )}
    </section>
  );
}

/** The registered component of one section id: draws that section of the open page, if it has one. */
export function sectionComponent(id: string) {
  function SettingsSectionView() {
    const { page } = useSettings();
    const section = sectionOf(page, id);
    return section ? <Section section={section} /> : null;
  }
  SettingsSectionView.displayName = `SettingsSection(${id})`;
  return SettingsSectionView;
}
