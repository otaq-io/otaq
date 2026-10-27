// Settings → About (d0071, d0076; *15 · Settings → Row · About & open-source licenses*, *… · groups & filter*): the
// Command tent on its app-icon tile, the name and line, the version with Copy version info, Otaq's own license
// (View license), the Open-source licenses dialog, Theme credits, and the trademark line. Every text comes from
// `settings.about`, `settings.license` and `settings.licenses` (`about.rs`, `licenses.rs`). Under the version: the
// update notice, Check for updates automatically and the Updates row (UPD-04, d0140; page 32 items 1–2;
// `updates.view`, `updates.rs`), then Use otaq in your own terminal with Add to my shell's PATH (page 32 item 5,
// `settings.about_path`).
import { type ReactNode, useEffect, useRef, useState } from "react";

import { useStore } from "../../app/store";
import { Button, Dialog, IconButton, Kbd, OtaqMark, PLAIN_TEXT, Toggle } from "../../components";
import type { AboutPathRow } from "../../generated/AboutPathRow";
import type { AboutView } from "../../generated/AboutView";
import type { LicensesDialog as LicensesView } from "../../generated/LicensesDialog";
import type { LicensesItem } from "../../generated/LicensesItem";
import type { OwnLicenseDialog } from "../../generated/OwnLicenseDialog";
import type { UpdateNotice } from "../../generated/UpdateNotice";
import type { UpdatesView } from "../../generated/UpdatesView";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { useSettings } from "./context";
import { fill, filterLicenses, groupCount, groupKey, resultLine, type ShownGroup } from "./licenses";
import { updatesClick, updatesStore, watchUpdates } from "./updates";

/** How long a Copy button says "Copied". */
const COPIED_MS = 2000;

/** A Copy button that says "Copied" for two seconds after a click. */
function CopyButton({ label, copied, text }: { label: string; copied: string; text: string }) {
  const [done, setDone] = useState(false);
  useEffect(() => {
    if (!done) return;
    const timer = setTimeout(() => {
      setDone(false);
    }, COPIED_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [done]);
  return (
    <Button
      variant="secondary"
      icon={done ? "check" : "copy"}
      label={done ? copied : label}
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setDone(true);
        });
      }}
    />
  );
}

function openSource(url: string) {
  void request("settings.open_source", { url });
}

/** *View license · dialog*: Otaq's own license text (FSL-1.1-MIT). */
function LicenseDialog({ view, onClose }: { view: OwnLicenseDialog; onClose: () => void }) {
  return (
    <Dialog
      title={view.title}
      hint="close"
      width={520}
      onConfirm={onClose}
      onCancel={onClose}
      head={<p className="about-dialog-description">{view.description}</p>}
      footer={
        <>
          <CopyButton label={view.copy_label} copied={view.copied_label} text={view.text} />
          <Button label={view.done_label} onClick={onClose} />
        </>
      }
    >
      <div className="about-license-text">
        {view.paragraphs.map((p, i) => (
          <p key={i} data-strong={i < 2 || undefined}>
            {p}
          </p>
        ))}
      </div>
    </Dialog>
  );
}

function ItemRow({ item, sourceLabel }: { item: LicensesItem; sourceLabel: string }) {
  return (
    <div className="about-item">
      <span className="about-item-name">{item.name}</span>
      <span className="about-item-version">{item.version}</span>
      <span className="about-item-repo">{item.source_text}</span>
      <IconButton
        icon="arrow-up-right"
        label={`${sourceLabel}: ${item.name}`}
        onClick={() => {
          openSource(item.source);
        }}
      />
    </div>
  );
}

function Group({
  view,
  shown,
  open,
  onToggle,
}: {
  view: LicensesView;
  shown: ShownGroup;
  open: boolean;
  onToggle: () => void;
}) {
  const [textOpen, setTextOpen] = useState(false);
  const { group, items } = shown;
  return (
    <div className="about-group" data-open={open || undefined}>
      <button type="button" className="about-group-header" aria-expanded={open} onClick={onToggle}>
        <Icon name={open ? "chevron-down" : "chevron-right"} />
        <span className="about-group-license">{group.license}</span>
        <span className="about-group-count">{groupCount(view, shown)}</span>
      </button>
      {open && (
        <>
          <div className="about-group-items">
            {items.map((item) => (
              <ItemRow key={`${item.name}@${item.version}`} item={item} sourceLabel={view.source_label} />
            ))}
            {group.note !== null && <p className="about-group-note">{group.note}</p>}
          </div>
          <div className="about-group-text">
            <button
              type="button"
              className="about-text-toggle"
              aria-expanded={textOpen}
              onClick={() => {
                setTextOpen((o) => !o);
              }}
            >
              <Icon name={textOpen ? "chevron-down" : "chevron-right"} size={13} />
              <span>{textOpen ? view.hide_text_label : view.show_text_label}</span>
            </button>
            {textOpen &&
              group.texts.map((text, i) => (
                <pre key={i} className="about-license-block">
                  {text}
                </pre>
              ))}
          </div>
        </>
      )}
    </div>
  );
}

/** *Open-source licenses · dialog*: the bundled things, then the Rust crates and npm packages by license. */
function LicensesDialog({
  view,
  onClose,
  onCredits,
}: {
  view: LicensesView;
  onClose: () => void;
  onCredits: () => void;
}) {
  const [query, setQuery] = useState("");
  // Folded by default; while filtering, every group with a match is shown open.
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const input = useRef<HTMLInputElement>(null);
  const shown = filterLicenses(view, query);

  // ⌘F / Ctrl+F focuses the filter.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        e.stopPropagation();
        input.current?.focus();
        input.current?.select();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
    };
  }, []);

  const clear = () => {
    setQuery("");
    setOpen(new Set());
    input.current?.focus();
  };
  const toggle = (key: string) => {
    setOpen((was) => {
      const next = new Set(was);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };
  const groups = (section: "rust" | "npm", list: readonly ShownGroup[]) => (
    <div className="about-groups">
      {list.map((g) => {
        const key = groupKey(section, g.group);
        return (
          <Group
            key={key}
            view={view}
            shown={g}
            open={shown.filtering || open.has(key)}
            onToggle={() => {
              toggle(key);
            }}
          />
        );
      })}
    </div>
  );
  const nothing = shown.filtering && shown.matches === 0 && shown.bundled.length === 0;

  return (
    <Dialog
      title={view.title}
      hint={view.close_hint}
      width={600}
      height="min(calc(100vh - 80px), 780px)"
      onConfirm={onClose}
      onCancel={onClose}
      head={
        <div className="about-licenses-head">
          <p className="about-dialog-description">{view.description}</p>
          <label className="about-filter" data-filled={query !== "" || undefined}>
            <Icon name="search" />
            <input
              {...PLAIN_TEXT}
              ref={input}
              type="text"
              value={query}
              placeholder={view.filter_placeholder}
              aria-label={view.filter_placeholder}
              onChange={(e) => {
                setQuery(e.target.value);
              }}
            />
            {query === "" ? (
              <Kbd keys={view.filter_keys} />
            ) : (
              <IconButton icon="x" label={view.clear_label} onClick={clear} />
            )}
          </label>
        </div>
      }
      hints={
        <>
          <span className="about-hint-gap" />
          <Kbd keys={view.filter_keys} />
          <span>{view.filter_hint}</span>
        </>
      }
      footer={
        <>
          <CopyButton label={view.copy_label} copied={view.copied_label} text={view.notices} />
          <Button label={view.done_label} onClick={onClose} />
        </>
      }
    >
      {nothing ? (
        <div className="about-empty">
          <Icon name="search" size={22} />
          <strong>{fill(view.no_match, { query: query.trim() })}</strong>
          <button type="button" className="settings-link" onClick={clear}>
            {view.clear_label}
          </button>
        </div>
      ) : (
        <>
          {shown.filtering && <p className="about-result">{resultLine(view, shown, query)}</p>}
          {shown.bundled.length > 0 && (
            <>
              <h3 className="about-heading">{view.bundled_heading}</h3>
              <div className="about-bundled">
                {shown.bundled.map((b) => (
                  <div key={b.project} className="about-bundled-row">
                    <div className="about-bundled-text">
                      <strong>{b.project}</strong>
                      <span>{b.source_text}</span>
                    </div>
                    <span className="about-chip">{b.license}</span>
                    {b.source === null ? (
                      <button type="button" className="settings-link" onClick={onCredits}>
                        {view.credits_label}
                      </button>
                    ) : (
                      <IconButton
                        icon="arrow-up-right"
                        label={`${view.source_label}: ${b.project}`}
                        onClick={() => {
                          if (b.source !== null) openSource(b.source);
                        }}
                      />
                    )}
                  </div>
                ))}
              </div>
            </>
          )}
          {shown.rust.length > 0 && (
            <>
              {!shown.filtering && <h3 className="about-heading">{view.rust_heading}</h3>}
              {groups("rust", shown.rust)}
            </>
          )}
          {shown.npm.length > 0 && (
            <>
              {!shown.filtering && <h3 className="about-heading">{view.npm_heading}</h3>}
              {groups("npm", shown.npm)}
            </>
          )}
        </>
      )}
    </Dialog>
  );
}

/**
 * *Use otaq in your own terminal* (page 32 item 5): the offer with Add to my shell's PATH, or what it did (added,
 * already on the PATH, or the line to paste when the file can't be written). Nothing is written until the click.
 */
function PathRow() {
  const [row, setRow] = useState<AboutPathRow | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void request("settings.about_path").then((v) => {
      setRow(v as AboutPathRow);
    });
  }, []);
  if (!row) return null;

  const add = () => {
    setBusy(true);
    void request("settings.about_path_add")
      .then((v) => {
        setRow(v as AboutPathRow);
      })
      .finally(() => {
        setBusy(false);
      });
  };

  if (row.tone === "offer") {
    return (
      <div className="settings-row about-row">
        <div className="settings-row-text">
          <span className="settings-row-label">{row.label}</span>
          {row.description !== null && <span className="settings-row-description">{row.description}</span>}
          {row.paste !== null && (
            <div className="about-path-paste">
              <code>{row.paste}</code>
              <CopyButton label={row.copy_label} copied={row.copied_label} text={row.paste} />
            </div>
          )}
        </div>
        <div className="settings-row-control">
          {row.button !== null && (
            <Button
              variant="secondary"
              icon="square-terminal"
              label={row.button}
              loading={busy}
              onClick={add}
            />
          )}
        </div>
      </div>
    );
  }
  const failed = row.tone === "failed";
  return (
    <div className="settings-row about-row about-path">
      <div className="settings-row-text">
        <span className="settings-row-label">{row.label}</span>
        <div className="about-path-result" data-failed={failed || undefined}>
          <span className="about-path-line">
            <Icon name={failed ? "circle-alert" : "check"} size={13} />
            <span>{row.result}</span>
          </span>
          {row.detail !== null && <span className="about-path-detail">{row.detail}</span>}
        </div>
        {row.paste !== null && (
          <div className="about-path-paste">
            <code>{row.paste}</code>
            <CopyButton label={row.copy_label} copied={row.copied_label} text={row.paste} />
          </div>
        )}
      </div>
    </div>
  );
}

function warn(what: string) {
  return (error: unknown) => {
    console.warn(`${what} failed:`, error);
  };
}

/** The notice at the top of About (page 32 item 2): ready, needs agents restarted, or the `.deb`'s available. */
export function UpdateNoticeBox({ notice }: { notice: UpdateNotice }) {
  const tone = notice.kind === "needs_agents_restarted" ? "paused" : "working";
  return (
    <div className="about-update-notice" data-tone={tone}>
      <div className="about-update-title">
        <span className="about-update-dot" />
        <strong>{notice.title}</strong>
      </div>
      <p>{notice.body}</p>
      <div className="about-update-actions">
        <button
          type="button"
          className="settings-link about-update-link"
          onClick={() => {
            updatesClick({ kind: "open", what: "whats_new" }).catch(warn("What's new"));
          }}
        >
          {notice.whats_new_label}
        </button>
        <span className="about-hint-gap" />
        <Button
          variant="ghost"
          label={notice.skip_label}
          onClick={() => {
            updatesClick({ kind: "skip" }).catch(warn("Skip this version"));
          }}
        />
        <Button
          variant={notice.primary ? "primary" : "secondary"}
          icon={notice.action === "download" ? "download" : "rotate-cw"}
          label={notice.action_label}
          disabled={!notice.enabled}
          onClick={() => {
            if (notice.action === "download") {
              updatesClick({ kind: "open", what: "download" }).catch(warn("Download"));
            } else {
              updatesClick({ kind: "restart" }).catch(warn("Restart now"));
            }
          }}
        />
      </div>
    </div>
  );
}

/** Check for updates automatically, and the Updates row with Check for updates and the last result (page 32 item 1). */
export function UpdateRows({ view }: { view: UpdatesView }) {
  const { auto, row } = view;
  const result = row.result;
  return (
    <>
      {auto !== null && (
        <div className="settings-row settings-row-switch about-row">
          <div className="settings-row-text">
            <span className="settings-row-label">{auto.label}</span>
            <span className="settings-row-description">{auto.description}</span>
          </div>
          <div className="settings-row-control">
            <Toggle
              on={auto.on}
              label={auto.label}
              showLabel={false}
              onChange={(on) => {
                updatesClick({ kind: "auto", on }).catch(warn("Check for updates automatically"));
              }}
            />
          </div>
        </div>
      )}
      <div className="settings-row about-row">
        <div className="settings-row-text">
          <span className="settings-row-label">{row.label}</span>
          {row.plain !== null && <span className="settings-row-description">{row.plain}</span>}
          {result !== null && (
            <div className="about-update-result" data-tone={result.tone}>
              <span className="about-update-line">
                <Icon name={result.icon} size={14} />
                <span>{result.text}</span>
              </span>
              {result.progress !== null && (
                <span className="about-update-progress">
                  <span className="about-update-bar">
                    {result.progress.permille !== null && (
                      <span style={{ width: `${(result.progress.permille / 10).toFixed(1)}%` }} />
                    )}
                  </span>
                  <span className="about-update-amount">{result.progress.text}</span>
                </span>
              )}
              {result.detail !== null && <span className="about-update-detail">{result.detail}</span>}
            </div>
          )}
        </div>
        {row.check !== null && (
          <div className="settings-row-control">
            <Button
              variant="secondary"
              icon="refresh-cw"
              label={row.check.label}
              disabled={!row.check.enabled}
              onClick={() => {
                updatesClick({ kind: "check" }).catch(warn("Check for updates"));
              }}
            />
          </div>
        )}
      </div>
    </>
  );
}

type Open = { kind: "license"; view: OwnLicenseDialog } | { kind: "licenses"; view: LicensesView } | null;

/** The About section, registered as the global page's last section. */
export function AboutSection() {
  const { act } = useSettings();
  const [view, setView] = useState<AboutView | null>(null);
  const [dialog, setDialog] = useState<Open>(null);
  const updates = useStore(updatesStore);
  useEffect(() => {
    void request("settings.about").then((v) => {
      setView(v as AboutView);
    });
  }, []);
  useEffect(() => watchUpdates(updatesStore.set), []);
  if (!view) return null;

  const row = (label: string, description: string, control: ReactNode) => (
    <div className="settings-row about-row">
      <div className="settings-row-text">
        <span className="settings-row-label">{label}</span>
        <span className="settings-row-description">{description}</span>
      </div>
      <div className="settings-row-control">{control}</div>
    </div>
  );

  return (
    <section className="settings-section about" data-section="about" aria-label={view.product}>
      <div className="about-identity">
        <span className="about-tile">
          <OtaqMark size={40} />
        </span>
        <div className="about-name">
          <strong>{view.product}</strong>
          <span>{view.tagline}</span>
        </div>
      </div>
      <div className="settings-row about-row">
        <div className="settings-row-text">
          <span className="settings-row-label about-version">
            {view.version}
            <code className="about-commit">{view.commit}</code>
          </span>
          <span className="settings-row-description">{view.version_description}</span>
          {view.built_from_source !== null && (
            <span className="settings-row-description">{view.built_from_source}</span>
          )}
        </div>
        <div className="settings-row-control">
          <CopyButton label={view.copy_label} copied={view.copied_label} text={view.version_info} />
        </div>
      </div>
      {updates?.notice && <UpdateNoticeBox notice={updates.notice} />}
      {updates && <UpdateRows view={updates} />}
      <PathRow />
      {row(
        view.copyright,
        view.license_description,
        <button
          type="button"
          className="settings-link"
          onClick={() => {
            void request("settings.license").then((v) => {
              setDialog({ kind: "license", view: v as OwnLicenseDialog });
            });
          }}
        >
          {view.view_license_label}
        </button>,
      )}
      {row(
        view.licenses_label,
        view.licenses_description,
        <Button
          variant="secondary"
          icon="scroll-text"
          label={view.view_licenses_label}
          onClick={() => {
            void request("settings.licenses").then((v) => {
              setDialog({ kind: "licenses", view: v as LicensesView });
            });
          }}
        />,
      )}
      {row(
        view.credits_label,
        view.credits_description,
        <Button
          variant="secondary"
          icon="scroll-text"
          label={view.view_credits_label}
          onClick={() => {
            act("credits");
          }}
        />,
      )}
      <p className="about-trademark">{view.trademark}</p>
      {dialog?.kind === "license" && (
        <LicenseDialog
          view={dialog.view}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === "licenses" && (
        <LicensesDialog
          view={dialog.view}
          onClose={() => {
            setDialog(null);
          }}
          onCredits={() => {
            setDialog(null);
            act("credits");
          }}
        />
      )}
    </section>
  );
}
