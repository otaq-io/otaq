// Settings → Plugins and a project's Plugins section (the otaq-marketplace README): installed plugins with their badge
// (Official, Community, Local), the marketplaces' offer, the consent screen before every install and update, the
// global defaults, and the CLIs' Log in in a short terminal under the plugin. A project turns a plugin on with its own
// values and auto-pull. The words are the service's (`api/plugins.rs`, `PluginTexts`).
import { useCallback, useEffect, useState } from "react";

import { Button, Checkbox, Dialog, TextArea, TextInput } from "../../components";
import type { PluginConsent } from "../../generated/PluginConsent";
import type { PluginImported } from "../../generated/PluginImported";
import type { PluginInstalled } from "../../generated/PluginInstalled";
import type { PluginLabel } from "../../generated/PluginLabel";
import type { PluginRequirement } from "../../generated/PluginRequirement";
import type { PluginSetting } from "../../generated/PluginSetting";
import type { PluginsPage } from "../../generated/PluginsPage";
import type { PluginTerminalOpened } from "../../generated/PluginTerminalOpened";
import type { PluginTexts } from "../../generated/PluginTexts";
import type { ProjectPlugin } from "../../generated/ProjectPlugin";
import type { ProjectPluginsView } from "../../generated/ProjectPluginsView";
import { asRequestError, watchView } from "../../service";
import { pickFolder, request } from "../../tauri";
import { RowTerminal, useRowTerminals } from "../onboarding/RowTerminal";
import "./plugins.css";

type Values = Record<string, unknown>;

/** A plugin's badge. */
export function PluginBadge({ label, texts }: { label: PluginLabel; texts: PluginTexts }) {
  return (
    <span className="plugin-badge" data-label={label} title={texts.label_notes[label]}>
      {texts.labels[label]}
    </span>
  );
}

/** A map setting as text: one `name = value` per line. */
export function mapText(value: unknown): string {
  if (value === null || typeof value !== "object") return "";
  return Object.entries(value as Record<string, unknown>)
    .map(([k, v]) => `${k} = ${String(v)}`)
    .join("\n");
}

export function parseMapText(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const at = line.indexOf("=");
    if (at <= 0) continue;
    const key = line.slice(0, at).trim();
    const value = line.slice(at + 1).trim();
    if (key !== "" && value !== "") out[key] = value;
  }
  return out;
}

/** The text a field shows for a value (strings as they are, maps as lines). */
function shown(setting: PluginSetting, value: unknown): string {
  if (setting.type === "map") return mapText(value);
  return typeof value === "string" ? value : "";
}

/** A plugin's settings form; `fallback` is what an empty field uses (its placeholder). */
function SettingsForm({
  settings,
  values,
  fallback,
  scope,
  problems,
  onChange,
}: {
  settings: PluginSetting[];
  values: Values;
  fallback: Values;
  scope: "global" | "project";
  problems: [string, string][];
  onChange: (values: Values) => void;
}) {
  return (
    <div className="plugin-form">
      {settings
        .filter((s) => scope === "project" || s.scope !== "project")
        .map((s) => {
          const problem = problems.find(([k]) => k === s.key)?.[1];
          const placeholder = shown(s, fallback[s.key]) || s.placeholder || "";
          const text = shown(s, values[s.key]);
          const set = (t: string) => {
            onChange({ ...values, [s.key]: s.type === "map" ? parseMapText(t) : t });
          };
          return (
            <label key={s.key} className="plugin-field">
              <span className="plugin-field-label">
                {s.label}
                {s.required && " *"}
              </span>
              {s.type === "map" ? (
                <TextArea value={text} placeholder={placeholder} onChange={set} label={s.label} mono />
              ) : (
                <TextInput value={text} placeholder={placeholder} onChange={set} label={s.label} mono />
              )}
              {problem !== undefined ? (
                <span className="settings-problem">{problem}</span>
              ) : (
                s.help && <span className="plugin-field-help">{s.help}</span>
              )}
            </label>
          );
        })}
    </div>
  );
}

/** The consent screen: what the plugin runs, its badge, license and source; nothing installs before Agree. */
export function ConsentDialog({
  consent,
  texts,
  busy,
  onAgree,
  onCancel,
}: {
  consent: PluginConsent;
  texts: PluginTexts;
  busy: boolean;
  onAgree: () => void;
  onCancel: () => void;
}) {
  const update = consent.installed !== null && consent.installed !== consent.version;
  return (
    <Dialog
      title={`${texts.consent_title} ${consent.name} ${consent.version}`}
      width={560}
      onCancel={onCancel}
      onConfirm={onAgree}
      footer={
        <>
          <Button variant="secondary" label={texts.cancel} onClick={onCancel} />
          <Button label={texts.consent_confirm} loading={busy} onClick={onAgree} />
        </>
      }
    >
      <div className="plugin-consent">
        <p>
          <PluginBadge label={consent.label} texts={texts} /> {texts.label_notes[consent.label]}
        </p>
        <p>{texts.consent_intro}</p>
        <h4>{texts.consent_requires}</h4>
        <p>{consent.requires.join(", ")}</p>
        <h4>{texts.consent_commands}</h4>
        <ul className="plugin-commands">
          {consent.commands.map((c) => (
            <li key={c}>
              <code>{c}</code>
            </li>
          ))}
        </ul>
        {consent.capabilities.includes("auto_pull") && <p>{texts.consent_polls}</p>}
        <p>
          {texts.consent_license}: <code>{consent.license}</code>
        </p>
        <p>
          {texts.consent_source}:{" "}
          {consent.source_url !== null ? (
            <a href={consent.source_url} target="_blank" rel="noreferrer">
              {consent.source}
            </a>
          ) : (
            consent.source
          )}
        </p>
        {update && consent.diff_url !== null && (
          <p>
            <a href={consent.diff_url} target="_blank" rel="noreferrer">
              {texts.consent_changes} ({consent.installed} → {consent.version})
            </a>
          </p>
        )}
        {consent.warnings.length > 0 && (
          <div className="plugin-warnings" role="alert">
            <h4>{texts.consent_warnings}</h4>
            <ul>
              {consent.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Dialog>
  );
}

/** What the consent dialog installs when agreed. */
type Pending = { consent: PluginConsent } & (
  { kind: "marketplace"; marketplace: string } | { kind: "local"; folder: string }
);

function Requirements({
  plugin,
  texts,
  page,
}: {
  plugin: PluginInstalled;
  texts: PluginTexts;
  page: PluginsPage;
}) {
  const [rows, setRows] = useState<PluginRequirement[] | null>(null);
  const check = useCallback(
    () =>
      request("plugins.check", { id: plugin.id }).then((r) => {
        setRows(r as PluginRequirement[]);
      }),
    [plugin.id],
  );
  useEffect(() => {
    void check();
  }, [check]);
  // Whether the rows still offer `key` (`gh:login`, `az:extension`) after its terminal ended.
  const terminals = useRowTerminals((key) => {
    const [id, kind] = key.split(":");
    const r = (rows ?? []).find((row) => row.id === id);
    if (r === undefined) return false;
    return kind === "login" ? r.logged_in === false : r.extension_added === false;
  }, check);
  if (rows === null) return null;
  return (
    <ul className="plugin-requirements">
      {rows.map((r) => (
        <li key={r.id} data-ready={ready(r) || undefined}>
          <span className="plugin-requirement-name">{r.name}</span>
          <span className="plugin-requirement-state">{stateOf(r, texts)}</span>
          {!r.installed && r.install_hint !== null && <code>{r.install_hint}</code>}
          {r.extension_added === false && r.extension_install !== null && (
            <Button
              compact
              variant="secondary"
              icon="arrow-down-to-line"
              label={texts.add_extension}
              onClick={() => {
                terminals.start(r.id, `${r.id}:extension`, () =>
                  request("plugins.terminal", { id: plugin.id, requirement: r.id, extension: true }).then(
                    (o) => ({
                      session: (o as PluginTerminalOpened).session,
                      title: texts.add_extension,
                    }),
                  ),
                );
              }}
            />
          )}
          {r.logged_in === false && r.login !== null && (
            <Button
              compact
              variant="secondary"
              icon="square-terminal"
              label={texts.log_in}
              onClick={() => {
                terminals.start(r.id, `${r.id}:login`, () =>
                  request("plugins.terminal", { id: plugin.id, requirement: r.id, extension: false }).then(
                    (o) => ({
                      session: (o as PluginTerminalOpened).session,
                      title: texts.log_in,
                    }),
                  ),
                );
              }}
            />
          )}
          <span className="plugin-requirement-terminal">
            <RowTerminal row={r.id} texts={page.terminal} terminals={terminals} />
          </span>
        </li>
      ))}
    </ul>
  );
}

function ready(r: PluginRequirement): boolean {
  return r.installed && r.logged_in !== false && r.extension_added !== false;
}

function stateOf(r: PluginRequirement, texts: PluginTexts): string {
  if (!r.installed) return texts.not_installed;
  if (r.extension_added === false) return texts.extension_missing;
  if (r.logged_in === false) return texts.not_logged_in;
  return texts.ready;
}

function InstalledRow({
  plugin,
  page,
  onPage,
  onError,
  onUpdate,
}: {
  plugin: PluginInstalled;
  page: PluginsPage;
  onPage: (p: PluginsPage) => void;
  onError: (e: string | null) => void;
  onUpdate: (id: string, version: string) => void;
}) {
  const texts = page.texts;
  const [defaults, setDefaults] = useState<Values>(plugin.defaults);
  const [open, setOpen] = useState(false);
  const run = (method: string, params: unknown) => {
    request(method, params).then(
      (p) => {
        onPage(p as PluginsPage);
        onError(null);
      },
      (e: unknown) => {
        onError(asRequestError(e).message);
      },
    );
  };
  const hasGlobal = plugin.settings.some((s) => s.scope !== "project");
  return (
    <li className="plugin-row">
      <div className="plugin-row-head">
        <span className="plugin-name">{plugin.name}</span>
        <span className="plugin-version">{plugin.version}</span>
        <PluginBadge label={plugin.label} texts={texts} />
        <span className="plugin-license">{plugin.license}</span>
        <span className="plugin-spacer" />
        {plugin.update !== null && (
          <Button
            compact
            label={`${texts.update} ${plugin.update}`}
            onClick={() => {
              if (plugin.update !== null) onUpdate(plugin.id, plugin.update);
            }}
          />
        )}
        <Button
          compact
          variant="secondary"
          label={open ? texts.cancel : texts.check}
          onClick={() => {
            setOpen(!open);
          }}
        />
        <Button
          compact
          variant="secondary"
          icon="trash-2"
          label={texts.remove}
          onClick={() => {
            run("plugins.remove", { id: plugin.id });
          }}
        />
      </div>
      <p className="plugin-description">{plugin.description}</p>
      <p className="plugin-source">{plugin.source}</p>
      {plugin.problem !== null && <p className="settings-problem">{plugin.problem}</p>}
      {plugin.warnings.length > 0 && (
        <ul className="plugin-warnings">
          {plugin.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
      {open && (
        <>
          <Requirements plugin={plugin} texts={texts} page={page} />
          {hasGlobal && (
            <div className="plugin-defaults">
              <h4>{texts.defaults_heading}</h4>
              <p className="plugin-field-help">{texts.defaults_hint}</p>
              <SettingsForm
                settings={plugin.settings}
                values={defaults}
                fallback={Object.fromEntries(plugin.settings.map((s) => [s.key, s.default]))}
                scope="global"
                problems={[]}
                onChange={setDefaults}
              />
              <Button
                compact
                label={texts.save}
                onClick={() => {
                  run("plugins.defaults_set", { id: plugin.id, values: defaults });
                }}
              />
            </div>
          )}
        </>
      )}
    </li>
  );
}

export function PluginsSection() {
  const [page, setPage] = useState<PluginsPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [url, setUrl] = useState("");
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(
    () =>
      watchView(
        () => request("plugins.view") as Promise<PluginsPage>,
        ["plugins_changed"],
        (state) => {
          if (state.view) setPage(state.view);
          setError(state.error?.message ?? null);
        },
      ),
    [],
  );
  const fail = (e: unknown) => {
    setBusy(false);
    setError(asRequestError(e).message);
  };
  const onPage = (p: unknown) => {
    setBusy(false);
    setPage(p as PluginsPage);
    setError(null);
  };
  const askMarketplace = (marketplace: string, id: string, version: string | null) => {
    setBusy(true);
    request("plugins.consent", { marketplace, id, version }).then((c) => {
      setBusy(false);
      setPending({ kind: "marketplace", marketplace, consent: c as PluginConsent });
    }, fail);
  };
  const agree = () => {
    if (pending === null) return;
    setBusy(true);
    const c = pending.consent;
    const sent =
      pending.kind === "marketplace"
        ? request("plugins.install", {
            marketplace: pending.marketplace,
            id: c.id,
            version: c.version,
            digest: c.digest,
          })
        : request("plugins.add_local", { folder: pending.folder, digest: c.digest });
    sent.then(
      (p) => {
        setPending(null);
        onPage(p);
      },
      (e: unknown) => {
        setPending(null);
        fail(e);
      },
    );
  };
  if (page === null) return error !== null ? <p className="settings-error">{error}</p> : null;
  const texts = page.texts;
  const marketplaceOf = (id: string) =>
    page.available.find((a) => a.id === id)?.marketplace ?? page.marketplaces[0]?.id ?? "";
  const offered = page.available.filter((a) => a.installed === null);
  return (
    <div className="plugins">
      <p className="plugins-intro">{texts.intro}</p>
      {error !== null && (
        <p className="settings-error" role="alert">
          {error}
        </p>
      )}
      <h3 className="settings-section-title">{texts.installed_heading}</h3>
      {page.installed.length === 0 ? (
        <p className="plugin-empty">{texts.none_installed}</p>
      ) : (
        <ul className="plugin-list">
          {page.installed.map((p) => (
            <InstalledRow
              key={`${p.id}-${p.version}`}
              plugin={p}
              page={page}
              onPage={onPage}
              onError={setError}
              onUpdate={(id, version) => {
                askMarketplace(marketplaceOf(id), id, version);
              }}
            />
          ))}
        </ul>
      )}
      <h3 className="settings-section-title">{texts.browse_heading}</h3>
      {offered.length === 0 ? (
        <p className="plugin-empty">{texts.none_available}</p>
      ) : (
        <ul className="plugin-list">
          {offered.map((a) => (
            <li key={`${a.marketplace}-${a.id}`} className="plugin-row">
              <div className="plugin-row-head">
                <span className="plugin-name">{a.name}</span>
                <span className="plugin-version">{a.version}</span>
                <PluginBadge label={a.label} texts={texts} />
                <span className="plugin-license">{a.license}</span>
                <span className="plugin-spacer" />
                <Button
                  compact
                  label={texts.install}
                  loading={busy}
                  onClick={() => {
                    askMarketplace(a.marketplace, a.id, a.version);
                  }}
                />
              </div>
              <p className="plugin-description">{a.description}</p>
            </li>
          ))}
        </ul>
      )}
      <h3 className="settings-section-title">{texts.marketplaces_heading}</h3>
      <ul className="plugin-marketplaces">
        {page.marketplaces.map((m) => (
          <li key={m.id}>
            <PluginBadge label={m.official ? "official" : "community"} texts={texts} />
            <span>{m.url}</span>
            {m.error !== null && <span className="settings-problem">{m.error}</span>}
            {!m.official && (
              <Button
                compact
                variant="secondary"
                label={texts.remove}
                onClick={() => {
                  request("plugins.marketplace_remove", { id: m.id }).then(onPage, fail);
                }}
              />
            )}
          </li>
        ))}
      </ul>
      <div className="plugin-add">
        <TextInput value={url} placeholder={texts.add_marketplace_placeholder} onChange={setUrl} mono />
        <Button
          variant="secondary"
          label={texts.add_marketplace}
          disabled={url.trim() === ""}
          onClick={() => {
            request("plugins.marketplace_add", { url }).then((p) => {
              setUrl("");
              onPage(p);
            }, fail);
          }}
        />
        <Button
          variant="secondary"
          icon="folder-open"
          label={texts.add_local}
          onClick={() => {
            void pickFolder(texts.add_local).then((folder) => {
              if (folder === null) return;
              request("plugins.consent_local", { folder }).then((c) => {
                setPending({ kind: "local", folder, consent: c as PluginConsent });
              }, fail);
            });
          }}
        />
        <Button
          variant="secondary"
          icon="rotate-ccw"
          label={texts.refresh}
          onClick={() => {
            request("plugins.refresh_index").then(onPage, fail);
          }}
        />
      </div>
      {pending !== null && (
        <ConsentDialog
          consent={pending.consent}
          texts={texts}
          busy={busy}
          onAgree={agree}
          onCancel={() => {
            setPending(null);
          }}
        />
      )}
    </div>
  );
}

function ProjectPluginRow({
  project,
  plugin,
  texts,
  onView,
  onError,
}: {
  project: string;
  plugin: ProjectPlugin;
  texts: PluginTexts;
  onView: (v: ProjectPluginsView) => void;
  onError: (e: string | null) => void;
}) {
  const [values, setValues] = useState<Values>(plugin.values);
  // Ticking the box only marks it on: Save sends it with the values (a required field may still be empty). Turning
  // it off saves at once.
  const [enabled, setEnabled] = useState(plugin.enabled);
  const [autoPull, setAutoPull] = useState(plugin.auto_pull);
  const [minutes, setMinutes] = useState(String(plugin.pull_minutes));
  const [pulled, setPulled] = useState<string | null>(null);
  const save = (enabled: boolean) => {
    request("plugins.project_set", {
      project,
      plugin: plugin.plugin,
      enabled,
      values,
      auto_pull: autoPull,
      pull_minutes: Number.parseInt(minutes, 10) || 15,
    }).then(
      (v) => {
        onView(v as ProjectPluginsView);
        onError(null);
      },
      (e: unknown) => {
        onError(asRequestError(e).message);
      },
    );
  };
  return (
    <li className="plugin-row">
      <div className="plugin-row-head">
        <Checkbox
          checked={enabled}
          label={`${plugin.name} · ${texts.enable}`}
          onChange={(on) => {
            setEnabled(on);
            if (!on && plugin.enabled) save(false);
          }}
        />
        <PluginBadge label={plugin.label} texts={texts} />
      </div>
      <SettingsForm
        settings={plugin.settings}
        values={values}
        fallback={plugin.defaults}
        scope="project"
        problems={enabled ? plugin.problems : []}
        onChange={setValues}
      />
      {plugin.can_auto_pull && (
        <div className="plugin-pull">
          <Checkbox checked={autoPull} label={texts.auto_pull} onChange={setAutoPull} />
          <label className="plugin-field">
            <span className="plugin-field-label">{texts.every_minutes}</span>
            <TextInput value={minutes} onChange={setMinutes} mono label={texts.every_minutes} />
          </label>
          <p className="plugin-field-help">{texts.auto_pull_hint}</p>
          {plugin.last_pull_at !== null && (
            <p className="plugin-field-help">
              {texts.last_pull}: {new Date(plugin.last_pull_at).toLocaleString()}
            </p>
          )}
          {plugin.last_pull_error !== null && <p className="settings-problem">{plugin.last_pull_error}</p>}
          {pulled !== null && <p className="plugin-field-help">{pulled}</p>}
        </div>
      )}
      <div className="plugin-actions">
        <Button
          compact
          label={texts.save}
          onClick={() => {
            save(enabled);
          }}
        />
        {plugin.enabled && plugin.can_auto_pull && (
          <Button
            compact
            variant="secondary"
            label={texts.pull_now}
            onClick={() => {
              request("plugins.pull_now", { project, plugin: plugin.plugin }).then(
                (r) => {
                  const imported = r as PluginImported;
                  setPulled(
                    imported.created.map((t) => `${t.key} ${t.title}`).join(", ") || texts.up_to_date,
                  );
                },
                (e: unknown) => {
                  onError(asRequestError(e).message);
                },
              );
            }}
          />
        )}
      </div>
    </li>
  );
}

export function ProjectPluginsSection({ project }: { project: string }) {
  const [view, setView] = useState<ProjectPluginsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(
    () =>
      watchView(
        () => request("plugins.project", { project }) as Promise<ProjectPluginsView>,
        ["plugins_changed"],
        (state) => {
          if (state.view) setView(state.view);
          setError(state.error?.message ?? null);
        },
      ),
    [project],
  );
  if (view === null) return error !== null ? <p className="settings-error">{error}</p> : null;
  return (
    <div className="plugins">
      <p className="plugins-intro">
        {view.plugins.length === 0 ? view.texts.project_none : view.texts.project_intro}
      </p>
      {error !== null && (
        <p className="settings-error" role="alert">
          {error}
        </p>
      )}
      <ul className="plugin-list">
        {view.plugins.map((p) => (
          <ProjectPluginRow
            key={`${p.plugin}-${String(p.enabled)}-${JSON.stringify(p.values)}`}
            project={project}
            plugin={p}
            texts={view.texts}
            onView={setView}
            onError={setError}
          />
        ))}
      </ul>
    </div>
  );
}
