// The gallery's dependencies (T-0463; *24 → Dependencies on the cards* `s24-09`, *Adding and removing* `s24-11`), as
// Rust words them (`gallery.dependencies`): SHOP-20's blocked card and SHOP-29's "Blocks SHOP-20", the sheet's
// DEPENDENCIES section and the Add picker with the rows for "SHOP-3". A blocked ticket can be started: it then waits
// at the top of the first stage, saying what it waits for.
import { useCallback } from "react";

import { TicketCard } from "../../cards";
import type { DependenciesGallery as GalleryView } from "../../generated/DependenciesGallery";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { DependenciesBody, PickerBody } from "./Dependencies";

const ignore = () => undefined;

export function DependenciesGallery() {
  const load = useCallback(() => request("gallery.dependencies") as Promise<GalleryView>, []);
  const { view, error } = useServiceView(load, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <div className="gallery-stack">
      <div className="gallery-two">
        <figure className="gallery-stack-tight" style={{ width: 220 }}>
          <figcaption className="gallery-caption">Blocked (DPV-01)</figcaption>
          <TicketCard card={view.blocked} />
        </figure>
        <figure className="gallery-stack-tight" style={{ width: 220 }}>
          <figcaption className="gallery-caption">Blocks SHOP-20</figcaption>
          <TicketCard card={view.blocker} />
        </figure>
      </div>
      <div className="gallery-two">
        <figure className="gallery-stack-tight overview" style={{ width: 640 }}>
          <figcaption className="gallery-caption">The sheet&apos;s DEPENDENCIES (DPV-04)</figcaption>
          <DependenciesBody ticket={view.section.key} view={view.section} />
        </figure>
        <figure className="gallery-stack-tight">
          <figcaption className="gallery-caption">Add: a ticket it waits for</figcaption>
          <div className="popover">
            <PickerBody
              search={view.section.search}
              rule={view.section.rule}
              query="SHOP-3"
              onQuery={ignore}
              rows={view.search}
              refusal={null}
              onPick={ignore}
            />
          </div>
        </figure>
      </div>
    </div>
  );
}
