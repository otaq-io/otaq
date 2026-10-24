// Area 01 · Components (T-0217): the confirm dialog every screen opens by id (`openOverlay("confirm", …)`), and
// the developer-only design gallery, opened over the app when it starts with `OTAQ_GALLERY=1`. The components
// themselves are in `ui/src/components/`.
import { openOverlay } from "../../app/overlays";
import type { OverlayProps, Screen } from "../../app/registry";
import { ConfirmDialog } from "../../components";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { Gallery } from "./Gallery";

function ConfirmOverlay({ params, close }: OverlayProps<"confirm">) {
  return (
    <ConfirmDialog
      title={params.title}
      body={
        params.rows === undefined ? (
          params.body
        ) : (
          <>
            <p className="dialog-text">{params.body}</p>
            <ul className="confirm-rows">
              {params.rows.map((row) => (
                <li key={row.text} className="confirm-row">
                  <Icon name={row.icon} size={13} />
                  <span>{row.text}</span>
                </li>
              ))}
            </ul>
          </>
        )
      }
      {...(params.confirmIcon === undefined ? {} : { confirmIcon: params.confirmIcon })}
      confirmLabel={params.confirmLabel}
      {...(params.cancelLabel === undefined ? {} : { cancelLabel: params.cancelLabel })}
      destructive={params.destructive}
      cancelDefault={params.cancelDefault ?? false}
      {...(params.extra === undefined
        ? {}
        : {
            extra: {
              label: params.extra.label,
              onClick: () => {
                close();
                void params.extra?.onClick();
              },
            },
          })}
      onConfirm={() => {
        close();
        void params.onConfirm();
      }}
      onCancel={close}
    />
  );
}

/** Opens the gallery when the app was started with `OTAQ_GALLERY=1` (`gallery.wanted`). */
export async function openGalleryIfWanted(ask: typeof request = request): Promise<boolean> {
  const wanted = await ask("gallery.wanted").then(
    (answer) => answer === true,
    () => false,
  );
  if (wanted) openOverlay("gallery");
  return wanted;
}

export const components: Screen = (app) => {
  app.overlays.register("confirm", { Component: ConfirmOverlay });
  app.overlays.register("gallery", { Component: Gallery });
  void openGalleryIfWanted();
};
