// Add project later: from the rail's +, ⌘P → Add project, the empty board's "Add project folder…" or a folder dropped
// on the window. Tools were checked already, so it is one dialog: the folder, the key, the base branch and a light
// Flow choice (Otaq default selected, Same as <project>, Suggest one); Esc or Cancel closes it. Adding selects the new
// project and closes the dialog onto its board; "Suggest one" opens Edit flow there while Noyan's agent drafts a flow.
import type { OverlayProps } from "../../app/registry";
import { Button, Dialog } from "../../components";
import { AddButton, AddProjectFields, useAddProject } from "./AddProjectForm";
import { added } from "./logic";

function AddProjectBody({ folder, close }: { folder: string | null; close: () => void }) {
  const state = useAddProject(false, folder, (key, flow) => {
    added(key, flow);
    close();
  });
  const { view } = state;
  return (
    <Dialog
      title={view?.title ?? ""}
      width={480}
      onConfirm={state.add}
      onCancel={close}
      footer={
        <>
          <span className="onboarding-spacer" />
          {view && <Button label={view.cancel} variant="ghost" onClick={close} />}
          <AddButton state={state} />
        </>
      }
    >
      <AddProjectFields state={state} />
    </Dialog>
  );
}

export function AddProjectDialog({ params, close }: OverlayProps<"add_project">) {
  const folder = params?.folder ?? null;
  // Another folder (dropped while it is open) starts the form again.
  return <AddProjectBody key={folder ?? ""} folder={folder} close={close} />;
}
