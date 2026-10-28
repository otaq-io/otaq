// An imported ticket's tracker issue in the editor (the otaq-marketplace README, "Ticket sources"): "Open PAY-12 in Jira ↗"
// (plugins write nothing back, so this is the way to update the issue), and Refresh from tracker. Fields edited only
// on the tracker update silently; fields both sides changed open a dialog to pick, field by field.
import { useCallback, useEffect, useState } from "react";

import { Button, Dialog } from "../../components";
import type { PluginTexts } from "../../generated/PluginTexts";
import type { ProjectPluginsView } from "../../generated/ProjectPluginsView";
import type { TrackerConflict } from "../../generated/TrackerConflict";
import type { TrackerLink } from "../../generated/TrackerLink";
import type { TrackerRefresh } from "../../generated/TrackerRefresh";
import { asRequestError, watchView } from "../../service";
import { request } from "../../tauri";

type Pick = "tracker" | "mine";

export function RefreshDialog({
  conflicts,
  texts,
  onApply,
  onCancel,
}: {
  conflicts: TrackerConflict[];
  texts: PluginTexts;
  onApply: (picks: Record<string, Pick>) => void;
  onCancel: () => void;
}) {
  const [picks, setPicks] = useState<Record<string, Pick>>(
    Object.fromEntries(conflicts.map((c) => [c.field, "mine"])),
  );
  return (
    <Dialog
      title={texts.refresh_title}
      width={720}
      onCancel={onCancel}
      onConfirm={() => {
        onApply(picks);
      }}
      footer={
        <>
          <Button variant="secondary" label={texts.cancel} onClick={onCancel} />
          <Button
            label={texts.save}
            onClick={() => {
              onApply(picks);
            }}
          />
        </>
      }
    >
      <p>{texts.refresh_intro}</p>
      {conflicts.map((c) => (
        <div key={c.field} className="tracker-conflict">
          <h4>{c.field}</h4>
          <div className="tracker-conflict-sides">
            {(["mine", "tracker"] as const).map((side) => (
              <button
                key={side}
                type="button"
                className="tracker-conflict-side"
                data-picked={picks[c.field] === side || undefined}
                onClick={() => {
                  setPicks({ ...picks, [c.field]: side });
                }}
              >
                <span className="tracker-conflict-label">
                  {side === "mine" ? texts.keep_mine : texts.use_tracker}
                </span>
                <pre>{side === "mine" ? c.local : c.remote}</pre>
              </button>
            ))}
          </div>
        </div>
      ))}
    </Dialog>
  );
}

export function TrackerRow({ project, ticket }: { project: string; ticket: string }) {
  const [link, setLink] = useState<TrackerLink | null>(null);
  const [texts, setTexts] = useState<PluginTexts | null>(null);
  const [conflicts, setConflicts] = useState<TrackerConflict[] | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(
    () =>
      watchView(
        () =>
          Promise.all([
            request("plugins.links", { project }) as Promise<TrackerLink[]>,
            request("plugins.project", { project }) as Promise<ProjectPluginsView>,
          ]),
        ["plugins_changed", "ticket_changed"],
        (state) => {
          if (!state.view) return;
          const [links, view] = state.view;
          setLink(links.find((l) => l.ticket === ticket) ?? null);
          setTexts(view.texts);
        },
      ),
    [project, ticket],
  );
  const refresh = useCallback(() => {
    setBusy(true);
    setNote(null);
    request("plugins.refresh_ticket", { key: ticket }).then(
      (r) => {
        setBusy(false);
        const outcome = r as TrackerRefresh;
        if (outcome.conflicts.length > 0) setConflicts(outcome.conflicts);
        else if (texts) setNote(outcome.updated.length > 0 ? texts.refreshed : texts.up_to_date);
      },
      (e: unknown) => {
        setBusy(false);
        setNote(asRequestError(e).message);
      },
    );
  }, [ticket, texts]);
  if (link === null || texts === null) return null;
  return (
    <div className="editor-field tracker-row">
      <div className="editor-label">
        <span>{link.tracker}</span>
        {link.parent_key !== null && (
          <span className="editor-hint">
            {texts.parent}{" "}
            {link.parent_url !== null ? (
              <a href={link.parent_url} target="_blank" rel="noreferrer">
                {link.parent_key}
              </a>
            ) : (
              link.parent_key
            )}
          </span>
        )}
      </div>
      <div className="tracker-row-actions">
        <a href={link.url} target="_blank" rel="noreferrer" className="tracker-open">
          {texts.open_in_tracker} {link.tracker}: {link.key} ↗
        </a>
        <Button
          compact
          variant="secondary"
          icon="refresh-cw"
          label={texts.refresh_ticket}
          loading={busy}
          onClick={refresh}
        />
        {note !== null && <span className="editor-hint">{note}</span>}
      </div>
      {conflicts !== null && (
        <RefreshDialog
          conflicts={conflicts}
          texts={texts}
          onCancel={() => {
            setConflicts(null);
          }}
          onApply={(picks) => {
            request("plugins.resolve", { key: ticket, picks }).then(
              () => {
                setConflicts(null);
                setNote(texts.refreshed);
              },
              (e: unknown) => {
                setConflicts(null);
                setNote(asRequestError(e).message);
              },
            );
          }}
        />
      )}
    </div>
  );
}
