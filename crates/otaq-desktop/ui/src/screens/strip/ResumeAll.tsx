// Resume all's confirmation (*10 → Resume all · confirm*): the interrupted sessions it resumes, with their tickets,
// and the start-interrupted ones it leaves out. Opened from the Needs you strip's Interrupted group for its project
// (T-0459) and from the "sessions were interrupted" banner for every project (T-0231); the confirm resumes, so nothing
// resumes without the click (d0045).
import { useCallback, useEffect } from "react";

import type { OverlayProps } from "../../app/registry";
import { ConfirmDialog } from "../../components";
import type { ResumeAllDialog } from "../../generated/ResumeAllDialog";
import type { StripResumeAllParams } from "../../generated/StripResumeAllParams";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

/** The confirm: the strip's project through `strip.resume_all`, every project through the banner's own request. */
export function resumeAll(project: string | undefined, ask: typeof request = request): Promise<unknown> {
  if (project === undefined) return ask("notices.resume_all");
  const params: StripResumeAllParams = { project };
  return ask("strip.resume_all", params);
}

/** The dialog for a loaded confirmation (also what the markup tests render). */
export function ResumeAllBody({
  dialog,
  project,
  close,
}: {
  dialog: ResumeAllDialog;
  project?: string;
  close: () => void;
}) {
  return (
    <ConfirmDialog
      title={dialog.title}
      hint={dialog.cancel_hint}
      body={
        <>
          <p className="dialog-text">{dialog.text}</p>
          <ul className="strip-resume-list">
            {dialog.included.map((t) => (
              <li key={t.key}>
                <Icon name="zap-off" size={13} />
                <span className="strip-resume-key">{t.key}</span>
                <span>{t.title}</span>
              </li>
            ))}
          </ul>
          {dialog.not_included !== null && (
            <p className="strip-resume-left-out">
              <Icon name="circle-pause" size={13} />
              <span>{dialog.not_included}</span>
            </p>
          )}
        </>
      }
      confirmLabel={dialog.confirm}
      confirmIcon="play"
      onConfirm={() => {
        close();
        resumeAll(project).catch(() => undefined);
      }}
      onCancel={close}
    />
  );
}

/** The `resume_all` overlay; it closes at once when nothing is left to resume. */
export function ResumeAll({ params, close }: OverlayProps<"resume_all">) {
  const project = params?.project;
  const load = useCallback(() => {
    const ask: StripResumeAllParams = project === undefined ? {} : { project };
    return request("strip.resume_all_confirm", ask) as Promise<ResumeAllDialog | null>;
  }, [project]);
  const { view } = useServiceView(load, ["needs_you_changed"]);
  useEffect(() => {
    if (view === null) close();
  }, [view, close]);
  return view ? (
    <ResumeAllBody dialog={view} close={close} {...(project !== undefined && { project })} />
  ) : null;
}
