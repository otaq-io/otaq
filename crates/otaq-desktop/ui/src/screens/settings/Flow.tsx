// The Flow card of the project's Settings page (FLW-06…FLW-11): Export and Edit flow with the ⋯ menu, the waiting
// change with Cancel change, the stages as a strip from Backlog to Done (what each does and who works in it), and who
// changed the flow and when with Go back to the previous flow. Everything it says comes from Rust (`settings.flow`,
// `settings.flow_presets`, `settings.flow_copy`); what the buttons do is in `flowActions.ts`.
import { useCallback, useEffect, useRef, useState } from "react";

import { AnchoredMenu, Button, Dialog, IconButton } from "../../components";
import type { FlowCopyMenu } from "../../generated/FlowCopyMenu";
import type { FlowSettings } from "../../generated/FlowSettings";
import type { FlowSettingsStage } from "../../generated/FlowSettingsStage";
import type { MenuItem } from "../../generated/MenuItem";
import type { PresetCard } from "../../generated/PresetCard";
import type { PresetDialog as PresetDialogView } from "../../generated/PresetDialog";
import type { RequestError } from "../../generated/RequestError";
import { asRequestError, watchView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { changeFlow, exportFlow, menuChoice, proposeFrom } from "./flowActions";

/** The service events after which the section is fetched again: the flow, its waiting change, the agents. */
const REFETCH_ON = ["flow_changed", "agents_changed", "projects_changed", "ticket_changed"];

/** One stage of the strip: its name, what it does, and who works in it (the model and the kind on hover). */
function StageStep({ row }: { row: FlowSettingsStage }) {
  const title = [row.kind, row.model].filter((t) => t !== null && t !== "").join(" · ");
  return (
    <li className="settings-flow-step" title={title}>
      <span className="settings-flow-name">
        <Icon name={row.icon} size={12} />
        {row.name}
      </span>
      <span className="settings-flow-kind">{row.caption}</span>
      <span className="settings-flow-agent">{row.agent}</span>
    </li>
  );
}

/** Backlog or Done: the strip's fixed ends. */
function End({ label }: { label: string }) {
  return (
    <li className="settings-flow-step" data-end>
      <span className="settings-flow-name">{label}</span>
    </li>
  );
}

/** One preset: its name and how it delivers, its line, the small diagram of its columns, its file. */
function Preset({ card, onPick }: { card: PresetCard; onPick: () => void }) {
  return (
    <button type="button" className="settings-preset" onClick={onPick}>
      <span className="settings-preset-head">
        <span className="settings-preset-name">{card.name}</span>
        <span className="settings-preset-delivers">{card.delivers}</span>
      </span>
      <span className="settings-preset-line">{card.line}</span>
      <span className="settings-preset-columns">
        {card.columns.map((column, i) => (
          <span key={`${column.label}-${String(i)}`} className="settings-preset-column">
            <Icon name={column.icon} size={12} />
            <span>{column.label}</span>
          </span>
        ))}
      </span>
      <span className="settings-preset-file">
        <Icon name="file-text" size={12} />
        {card.file}
      </span>
    </button>
  );
}

/** Use a preset… (*Row · Use a preset*): picking one opens the flow editor on it. */
export function PresetDialog({
  dialog,
  onPick,
  onCancel,
}: {
  dialog: PresetDialogView;
  onPick: (file: string) => void;
  onCancel: () => void;
}) {
  return (
    <Dialog
      title={dialog.title}
      width={720}
      onCancel={onCancel}
      footer={<Button variant="ghost" label={dialog.cancel} onClick={onCancel} />}
    >
      <div className="settings-presets">
        {dialog.presets.map((card) => (
          <Preset
            key={card.file}
            card={card}
            onPick={() => {
              onPick(card.file);
            }}
          />
        ))}
      </div>
    </Dialog>
  );
}

/** The copy menu's items: one per other project. */
function copyItems(menu: FlowCopyMenu): MenuItem[] {
  return [{ ...item("", menu.title), header: true }, ...menu.projects.map((p) => item(p.key, p.name))];
}

function item(id: string, label: string): MenuItem {
  return {
    id,
    label,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: false,
    header: false,
  };
}

/** The section as the view says. */
export function FlowSectionBody({
  section,
  onCancelChange,
  onChange,
  onExport,
  onBack,
  onPreset,
  onCopy,
  onMenu,
}: {
  section: FlowSettings;
  onCancelChange: () => void;
  onChange: () => void;
  onExport: () => void;
  onBack: () => void;
  onPreset: () => void;
  onCopy: (project: string) => void;
  onMenu: (id: string) => void;
}) {
  const more = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [copy, setCopy] = useState<FlowCopyMenu | null>(null);
  return (
    <section className="settings-section settings-flow" data-section="flow" aria-label={section.title}>
      <div className="settings-flow-title">
        <div className="settings-flow-heading">
          <h3 className="settings-section-title">{section.title}</h3>
          <p className="settings-intro">{section.intro}</p>
        </div>
        <div className="settings-flow-actions">
          <Button
            variant="secondary"
            compact
            label={section.export.label}
            icon={section.export.icon ?? undefined}
            onClick={onExport}
          />
          <Button
            compact
            label={section.change.label}
            icon={section.change.icon ?? undefined}
            disabled={section.change.disabled}
            onClick={onChange}
          />
          <IconButton
            ref={more}
            icon="ellipsis"
            label={section.menu_label}
            onClick={() => {
              setMenuOpen((o) => !o);
            }}
          />
          {copy !== null && (
            <AnchoredMenu
              anchor={more}
              align="end"
              label={copy.title}
              items={copyItems(copy)}
              onChoose={(key) => {
                setCopy(null);
                onCopy(key);
              }}
              onClose={(how) => {
                setCopy(null);
                if (how === "key") more.current?.focus();
              }}
            />
          )}
          {menuOpen && (
            <AnchoredMenu
              anchor={more}
              align="end"
              label={section.menu_label}
              items={section.menu}
              onChoose={(id) => {
                setMenuOpen(false);
                if (id === "preset") onPreset();
                else if (id === "copy")
                  request("settings.flow_copy", { project: section.project }).then(
                    (menu) => {
                      setCopy(menu as FlowCopyMenu);
                    },
                    () => undefined,
                  );
                else onMenu(id);
              }}
              onClose={(how) => {
                setMenuOpen(false);
                if (how === "key") more.current?.focus();
              }}
            />
          )}
        </div>
      </div>
      {section.waiting !== null && (
        <div className="settings-flow-waiting" role="status">
          <Icon name="triangle-alert" size={14} />
          <span className="settings-flow-waiting-text">{section.waiting.text}</span>
          <Button compact variant="secondary" label={section.waiting.cancel_label} onClick={onCancelChange} />
        </div>
      )}
      {section.problem !== null ? (
        <p className="settings-problem" role="alert">
          {section.problem}
        </p>
      ) : (
        <ol className="settings-flow-strip" aria-label={section.title}>
          <End label={section.first} />
          {section.rows.map((row) => (
            <StageStep key={`${row.name}-${row.kind}`} row={row} />
          ))}
          <End label={section.last} />
        </ol>
      )}
      <p className="settings-flow-changed">
        <span>{section.changed}</span>
        {section.back !== null && (
          <button type="button" className="settings-link" onClick={onBack}>
            {section.back}
          </button>
        )}
      </p>
    </section>
  );
}

/** The registered section: loads `settings.flow` for its project and follows its changes. */
export function FlowSection({ project }: { project: string }) {
  const [section, setSection] = useState<FlowSettings | null>(null);
  const [error, setError] = useState<RequestError | null>(null);
  const [presets, setPresets] = useState<PresetDialogView | null>(null);
  const load = useCallback(() => request("settings.flow", { project }) as Promise<FlowSettings>, [project]);
  useEffect(
    () =>
      watchView(load, REFETCH_ON, (state) => {
        if (state.view) setSection(state.view);
        setError(state.error ?? null);
      }),
    [load],
  );
  const failed = (e: unknown) => {
    setError(asRequestError(e));
  };
  if (section === null) return error ? <p className="settings-problem">{error.message}</p> : null;
  return (
    <>
      {error && (
        <p className="settings-problem" role="alert">
          {error.message}
        </p>
      )}
      <FlowSectionBody
        section={section}
        onCancelChange={() => {
          request("settings.flow_cancel", { project }).then((fresh) => {
            setSection(fresh as FlowSettings);
            setError(null);
          }, failed);
        }}
        onChange={() => {
          changeFlow(section);
        }}
        onExport={() => {
          exportFlow(section).then(() => {
            setError(null);
          }, failed);
        }}
        onBack={() => {
          proposeFrom(project, { from: "previous" }).catch(failed);
        }}
        onPreset={() => {
          request("settings.flow_presets", { project }).then((d) => {
            setPresets(d as PresetDialogView);
          }, failed);
        }}
        onCopy={(from) => {
          proposeFrom(project, { from: "project", project: from }).catch(failed);
        }}
        onMenu={(id) => {
          menuChoice(section, id).then(() => {
            setError(null);
          }, failed);
        }}
      />
      {presets !== null && (
        <PresetDialog
          dialog={presets}
          onCancel={() => {
            setPresets(null);
          }}
          onPick={(file) => {
            setPresets(null);
            proposeFrom(project, { from: "preset", file }).catch(failed);
          }}
        />
      )}
    </>
  );
}
