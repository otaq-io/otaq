// The gallery's List view (T-0461; `s24-03`, `s24-04`, `s24-05`): the example's shop-web by stage (Done folded), by
// status, by agent, and by stage with Done shown and Hide empty stages on, each at the full width and narrowed as the
// agent panel leaves it (What's happening takes the difference). SHOP-22 is selected in the first. Nothing is sent;
// the fixtures are Rust's (`gallery.list_view`).
import { useCallback } from "react";

import type { ListGallery as Fixtures } from "../../generated/ListGallery";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { List, type ListHandlers } from "./List";

const ignore = () => undefined;
const HANDLERS: ListHandlers = {
  press: ignore,
  key: ignore,
  action: ignore,
  start: ignore,
  menu: ignore,
};

export function ListGallery() {
  const load = useCallback(() => request("gallery.list_view") as Promise<Fixtures>, []);
  const { view: g, error } = useServiceView(load, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!g) return null;
  return (
    <div className="gallery-stack">
      {g.states.flatMap((state, i) =>
        [1376, 976].map((width) => (
          <figure key={`${state.label}:${String(width)}`} className="gallery-stack-tight">
            <figcaption className="gallery-caption">
              {state.label} · {width === 1376 ? "1440, no panel" : "the agent panel open"}
            </figcaption>
            <div className="board" data-view="list" style={{ width, height: 560 }}>
              <List
                list={state.view}
                selected={i === 0 ? "SHOP-22" : null}
                lifted={null}
                verdict={ignore}
                handlers={HANDLERS}
                group={ignore}
                onView={ignore}
              />
            </div>
          </figure>
        )),
      )}
    </div>
  );
}
