// The gallery's ticket sheets (T-0464; *25 → Opening a ticket*, *Reviewing a ticket in your check*, *Epics*): the
// header, details line, path and tabs of `gallery.sheet`'s four tickets (needs you, your check, an epic's child,
// Backlog) at 1040 and 920 px, and one at full page. The tab bodies are left out (each tab's ticket draws its own).
import { useCallback, useState } from "react";

import { createRegistries, RegistriesContext } from "../../app/registry";
import type { SheetView } from "../../generated/SheetView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { SheetBody } from "./Sheet";

export function SheetGallery() {
  const load = useCallback(() => request("gallery.sheet") as Promise<SheetView[]>, []);
  const { view, error } = useServiceView(load, []);
  // Registries of their own: no tab bodies, and their keys don't reach the page's.
  const [app] = useState(createRegistries);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const [first] = view;
  return (
    <RegistriesContext value={app}>
      <div className="gallery-stack">
        {view.map((sheet) => (
          <figure key={sheet.key} className="gallery-stack-tight sheet-gallery">
            <figcaption className="gallery-caption">{sheet.key} · 1040 px</figcaption>
            <SheetBody view={sheet} tab={null} fullPage={false} width={1040} />
          </figure>
        ))}
        {first && (
          <>
            <figure className="gallery-stack-tight sheet-gallery">
              <figcaption className="gallery-caption">{first.key} · 920 px, beside Noyan</figcaption>
              <SheetBody view={first} tab="run" fullPage={false} width={920} />
            </figure>
            <figure className="gallery-stack-tight sheet-gallery sheet-gallery-full">
              <figcaption className="gallery-caption">{first.key} · full page</figcaption>
              <SheetBody view={first} tab={null} fullPage width={1040} />
            </figure>
          </>
        )}
      </div>
    </RegistriesContext>
  );
}
