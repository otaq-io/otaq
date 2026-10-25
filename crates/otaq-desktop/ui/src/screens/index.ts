// The screens index: one line per screen, `export { <area> } from "./<area>";`, nothing else. The page runs every
// screen exported here once at start (`registerScreens` in `main.tsx`). See the crate README, "How a screen plugs in";
// `_sample/` is the template (not listed: the registry tests add it).
export { activity } from "./activity";
export { askBar } from "./ask_bar";
export { board } from "./board";
export { components } from "./components";
export { dependencies } from "./dependencies";
export { editor } from "./editor";
export { files } from "./files";
export { flowEditor } from "./flow_editor";
export { notices } from "./notices";
export { onboarding } from "./onboarding";
export { overview } from "./overview";
export { palette } from "./palette";
export { agentPanel } from "./agent_panel";
export { pause } from "./pause";
export { pr } from "./pr";
export { recovery } from "./recovery";
export { review } from "./review";
export { sections } from "./sections";
export { session } from "./session";
export { settings } from "./settings";
export { sheet } from "./sheet";
export { shell } from "./shell";
export { shortcuts } from "./shortcuts";
export { strip } from "./strip";
export { terminals } from "./terminals";
export { titleBar } from "./title_bar";
