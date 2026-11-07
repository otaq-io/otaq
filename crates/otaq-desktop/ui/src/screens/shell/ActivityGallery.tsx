// The gallery's Running (T-0329): the title bar's usage chip and Running in each state (nothing at all while nothing
// runs and usage is fine) and the popover in each state, drawn from `gallery.activity`'s fixtures with the header's
// and the popover's own components. A button's click opens the live popover, as in the header.
import { useCallback } from "react";

import type { ActivityGallery as GalleryView } from "../../generated/ActivityGallery";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { ActivityBody } from "./ActivityPopover";
import { ActivityChip, UsageChip } from "./Header";

const ignore = () => undefined;

export function ActivityGallery() {
  const load = useCallback(() => request("gallery.activity") as Promise<GalleryView>, []);
  const { view, error } = useServiceView(load, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <div className="gallery-stack">
      <div className="activity-gallery-buttons">
        {view.buttons.map(([caption, button]) => (
          <div key={caption} className="activity-gallery-row">
            <span className="gallery-caption">{caption}</span>
            <span className="header">
              {button.warning && <UsageChip warning={button.warning} />}
              {button.shown && <ActivityChip activity={button} />}
              {!button.shown && !button.warning && (
                <span className="gallery-caption">nothing in the title bar</span>
              )}
            </span>
          </div>
        ))}
      </div>
      <div className="gallery-wrap gallery-wrap-wide activity-gallery-popovers">
        {view.popovers.map(([caption, popover]) => (
          <figure key={caption} className="gallery-stack-tight">
            <figcaption className="gallery-caption">{caption}</figcaption>
            <div className="popover activity-gallery-popover">
              <ActivityBody view={popover} onStep={ignore} onOpenTicket={ignore} onOpenTerminals={ignore} />
            </div>
          </figure>
        ))}
      </div>
    </div>
  );
}
