import { useEffect } from "react";

import { installContextMenu } from "./app/contextMenu";
import { Frame } from "./app/Frame";
import { KeyLog } from "./app/KeyLog";
import { installKeys } from "./app/keyboard";
import { type Registries, RegistriesContext } from "./app/registry";
import { SequenceHint } from "./app/SequenceHint";
import { TypingHint } from "./app/TypingHint";

// The page: the frame with the screens' registries, and the key dispatcher that runs their shortcut handlers. Why the
// service couldn't start shows in the banner area (T-0231, Q40).
export function App({ registries }: { registries: Registries }) {
  useEffect(() => installKeys(registries), [registries]);
  useEffect(() => installContextMenu(), []);
  return (
    <RegistriesContext value={registries}>
      <Frame />
      <SequenceHint />
      <TypingHint />
      <KeyLog />
    </RegistriesContext>
  );
}
