// Settings → Noyan (UX-70, refined 2026-11-16; canvas A-8): app settings' own Noyan section, right after Tools (Noyan
// isn't a tool). Its page holds only **Noyan starts on**: the shared Model and Effort field (`ModelEffort.tsx`), with
// the flows' model-and-effort pairs on top of its menu, the tool's effort row with Default first, and Set up <Tool>
// for a tool that isn't set up (which opens Settings → Tools). The page is `noyan_settings.view`; each change is
// `noyan_settings.edit`, answered with the fresh page. Every word is `noyan_settings.rs`'s.
import { useContext, useEffect, useState } from "react";

import type { NoyanSettingsEdit } from "../../generated/NoyanSettingsEdit";
import type { NoyanSettingsPage } from "../../generated/NoyanSettingsPage";
import { asRequestError, watchView } from "../../service";
import { request } from "../../tauri";
import { SettingsContext } from "./context";
import { ModelEffortField } from "./ModelEffort";
import type { ModelEffortPick } from "./model_effort_logic";

/** The edit a pick of Noyan starts on's menu sends (`noyan_settings.edit`). */
export function noyanEdit(pick: ModelEffortPick): NoyanSettingsEdit {
  return pick.kind === "pair"
    ? { edit: "pair", model: pick.model, ...(pick.effort !== null && { effort: pick.effort }) }
    : { edit: "model", model: pick.model };
}

/** The edit an Effort level of Noyan starts on sends; `null` is Default (the tool's own). */
export function effortEdit(effort: string | null): NoyanSettingsEdit {
  return effort === null ? { edit: "effort" } : { edit: "effort", effort };
}

/** The page as the view says: Noyan starts on, and nothing else. */
export function NoyanSettingsBody({
  page,
  onEdit,
  onSetUp,
}: {
  page: NoyanSettingsPage;
  /** A change to Noyan starts on; none in the gallery (the field then does nothing). */
  onEdit?: (edit: NoyanSettingsEdit) => void;
  /** Set up… / Set up <Tool>: Settings → Tools. */
  onSetUp?: (tool: string | null) => void;
}) {
  return (
    <section className="settings-section noyan-settings" data-section="noyan" aria-label={page.title}>
      <h3 className="settings-section-title">{page.title}</h3>
      <div className="noyan-starts-on" role="group" aria-label={page.label}>
        <span className="noyan-starts-on-label">{page.label}</span>
        <ModelEffortField
          view={page.picker}
          onPick={(pick) => {
            onEdit?.(noyanEdit(pick));
          }}
          onEffort={(effort) => {
            onEdit?.(effortEdit(effort));
          }}
          onSetUp={(tool) => {
            onSetUp?.(tool);
          }}
        />
      </div>
    </section>
  );
}

/** The registered section: the page from `noyan_settings.view`, again when Noyan starts on, a tool, a flow or the
 * projects change. */
export function NoyanSettingsSection() {
  const settings = useContext(SettingsContext);
  const [page, setPage] = useState<NoyanSettingsPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(
    () =>
      watchView(
        () => request("noyan_settings.view") as Promise<NoyanSettingsPage>,
        // Noyan starts on changed; a tool was set up (the field's marks); a flow or a project changed (the pairs).
        ["agents_changed", "agent_tools_changed", "flow_changed", "projects_changed"],
        (state) => {
          if (state.view) setPage(state.view);
          setError(state.error?.message ?? null);
        },
      ),
    [],
  );
  if (!page) return null;
  return (
    <>
      {error !== null && (
        <p className="settings-error" role="alert">
          {error}
        </p>
      )}
      <NoyanSettingsBody
        page={page}
        onEdit={(edit) => {
          request("noyan_settings.edit", edit).then(
            (fresh) => {
              setPage(fresh as NoyanSettingsPage);
              setError(null);
            },
            (e: unknown) => {
              setError(asRequestError(e).message);
            },
          );
        }}
        onSetUp={() => {
          settings?.openTopic?.("tools");
        }}
      />
    </>
  );
}
