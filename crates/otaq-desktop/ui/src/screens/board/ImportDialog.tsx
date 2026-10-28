// The board's Import dialog (the otaq-marketplace README, "Ticket sources"): New ticket ▾ → Import ▸ <plugin>. A search in
// the tracker's own language (the saved one to start), at most 100 open work items with checkboxes, already-imported
// ones greyed out, dismissed ones offered again; Import creates them as Backlog tickets, all or nothing.
import { useCallback, useEffect, useState } from "react";

import { Button, Checkbox, Dialog, TextInput } from "../../components";
import type { PluginImported } from "../../generated/PluginImported";
import type { PluginSearch } from "../../generated/PluginSearch";
import type { PluginTexts } from "../../generated/PluginTexts";
import type { ProjectPlugin } from "../../generated/ProjectPlugin";
import { asRequestError } from "../../service";
import { request } from "../../tauri";

export interface ImportDialogProps {
  project: string;
  plugin: ProjectPlugin;
  texts: PluginTexts;
  onDone: (imported: PluginImported) => void;
  onCancel: () => void;
}

export function ImportDialog({ project, plugin, texts, onDone, onCancel }: ImportDialogProps) {
  const saved = typeof plugin.values.query === "string" ? plugin.values.query : "";
  const fallback = typeof plugin.defaults.query === "string" ? plugin.defaults.query : "";
  const [query, setQuery] = useState(saved || fallback);
  const [result, setResult] = useState<PluginSearch | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // The state changes only once the answer is in; a click marks the search busy first.
  const search = useCallback(
    (q: string) =>
      request("plugins.search", { project, plugin: plugin.plugin, query: q.trim() === "" ? null : q }).then(
        (r) => {
          setBusy(false);
          setError(null);
          setResult(r as PluginSearch);
          setPicked(new Set());
        },
        (e: unknown) => {
          setBusy(false);
          setError(asRequestError(e).message);
        },
      ),
    [project, plugin.plugin],
  );
  useEffect(() => {
    void search(saved || fallback);
  }, [search, saved, fallback]);

  const importPicked = () => {
    if (picked.size === 0) return;
    setBusy(true);
    request("plugins.import", { project, plugin: plugin.plugin, ids: [...picked] }).then(
      (r) => {
        setBusy(false);
        onDone(r as PluginImported);
      },
      (e: unknown) => {
        setBusy(false);
        setError(asRequestError(e).message);
      },
    );
  };

  const label = `${texts.import_button} ${String(picked.size)}`;
  return (
    <Dialog
      title={`${texts.import_title} ${plugin.name}`}
      width={620}
      height="70vh"
      onCancel={onCancel}
      onConfirm={importPicked}
      head={
        <div className="import-search">
          <TextInput
            value={query}
            placeholder={plugin.query_label ?? texts.search_placeholder}
            onChange={setQuery}
            mono
            label={texts.search}
          />
          <Button
            variant="secondary"
            icon="search"
            label={texts.search}
            loading={busy && result === null}
            onClick={() => {
              setBusy(true);
              void search(query);
            }}
          />
        </div>
      }
      footer={
        <>
          <Button variant="secondary" label={texts.cancel} onClick={onCancel} />
          <Button label={label} disabled={picked.size === 0} loading={busy} onClick={importPicked} />
        </>
      }
    >
      {error !== null && (
        <p className="settings-error" role="alert">
          {error}
        </p>
      )}
      {result !== null && result.hits.length === 0 && <p className="import-empty">{texts.no_results}</p>}
      {result !== null && (
        <ul className="import-hits">
          {result.hits.map((hit) => {
            const imported = hit.ticket !== null;
            return (
              <li key={hit.issue.id} data-imported={imported || undefined}>
                <Checkbox
                  checked={picked.has(hit.issue.id)}
                  disabled={imported}
                  label={`${hit.issue.key}  ${hit.issue.title}`}
                  onChange={(on) => {
                    const next = new Set(picked);
                    if (on) next.add(hit.issue.id);
                    else next.delete(hit.issue.id);
                    setPicked(next);
                  }}
                />
                <span className="import-note">
                  {imported
                    ? `${texts.already_imported} · ${hit.ticket ?? ""}`
                    : hit.dismissed
                      ? texts.dismissed
                      : hit.issue.parent !== null
                        ? `${texts.parent} ${hit.issue.parent.key}`
                        : ""}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {result !== null && result.truncated && <p className="import-note">{texts.truncated}</p>}
      {result !== null && result.hidden > 0 && (
        <p className="import-note">
          {String(result.hidden)} {texts.hidden}
        </p>
      )}
    </Dialog>
  );
}
