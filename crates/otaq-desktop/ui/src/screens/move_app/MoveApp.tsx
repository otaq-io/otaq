// Move Otaq to Applications (*32 · item 6*, artboard s32-03; T-0579, UPD-06): a macOS release opened outside an
// Applications folder shows only this, filling the window: no board, no service behind it. Move to Applications
// copies the app there, opens the moved copy and quits this one; a failed move says why with Try again and Show in
// Finder. The words and which buttons show come from Rust (`move_app.view`).
import { useState } from "react";

import "./move_app.css";
import { Button, OtaqMark } from "../../components";
import type { MoveAppView } from "../../generated/MoveAppView";
import { asRequestError } from "../../service";
import { DRAG_REGION, request, windowAction } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { afterMove } from "./logic";

/** The window's body for `view`; the buttons call back (also what the markup tests render). */
export function MoveBody({
  view,
  busy,
  onQuit,
  onMove,
  onShowInFinder,
}: {
  view: MoveAppView;
  busy: boolean;
  onQuit: () => void;
  onMove: () => void;
  onShowInFinder: () => void;
}) {
  return (
    <div className="move-app">
      <div className="move-app-titlebar" {...DRAG_REGION}>
        <span>{view.window_title}</span>
      </div>
      <div className="move-app-body">
        <OtaqMark size={72} />
        <h1 className="move-app-title">{view.title}</h1>
        {view.error !== null && (
          <div className="move-app-error" role="alert">
            <Icon name="circle-alert" size={14} />
            <span>{view.error}</span>
          </div>
        )}
        <p className="move-app-text">{view.text}</p>
        <div className="move-app-actions">
          <Button label={view.quit} variant="ghost" disabled={busy} onClick={onQuit} />
          {view.try_again !== null && (
            <Button label={view.try_again} variant="secondary" loading={busy} onClick={onMove} />
          )}
          {view.move_to !== null && (
            <Button label={view.move_to} icon="folder-input" loading={busy} onClick={onMove} />
          )}
          {view.show_in_finder !== null && (
            <Button label={view.show_in_finder} disabled={busy} onClick={onShowInFinder} />
          )}
        </div>
      </div>
    </div>
  );
}

export function MoveApp({ initial }: { initial: MoveAppView }) {
  const [view, setView] = useState(initial);
  const [busy, setBusy] = useState(false);
  const quit = () => void windowAction("quit");
  const move = () => {
    setBusy(true);
    request("move_app.move")
      .then((answer) => {
        const outcome = afterMove(answer);
        if (outcome.quit) quit();
        else setView(outcome.view);
      })
      .catch((e: unknown) => {
        console.error(asRequestError(e).message);
      })
      .finally(() => {
        setBusy(false);
      });
  };
  const showInFinder = () => {
    request("move_app.show_in_finder").catch((e: unknown) => {
      console.error(asRequestError(e).message);
    });
  };
  return <MoveBody view={view} busy={busy} onQuit={quit} onMove={move} onShowInFinder={showInFinder} />;
}
