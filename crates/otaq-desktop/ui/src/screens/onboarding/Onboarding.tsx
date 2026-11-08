// First run (UI-08, UI-11, ONB-10): over the whole window, its own title bar with the step indicator (Welcome · Tools ·
// Project) between the OS's window controls, and one card per step. Welcome teaches the model with the board itself
// (the default flow's columns, who acts in each, the turquoise card) and one button; Tools is the agent tools and git
// (Check again, Continue once an agent tool is ready; a row's Install / Sign in… runs in a terminal under it); Project
// adds the first project. Adding it ends onboarding on the project's board, on Otaq default, with "Your first ticket"
// in Backlog. No Theme step (the theme follows the system) and no Flow step (Edit flow is offered after the first
// approve).
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { useRegistries } from "../../app/registry";
import { useStore } from "../../app/store";
import { Button } from "../../components";
import type { OnboardingColumn } from "../../generated/OnboardingColumn";
import type { OnboardingStepId } from "../../generated/OnboardingStepId";
import type { OnboardingTexts } from "../../generated/OnboardingTexts";
import type { OnboardingTitleBar } from "../../generated/OnboardingTitleBar";
import type { OnboardingTools } from "../../generated/OnboardingTools";
import type { OnboardingWelcome } from "../../generated/OnboardingWelcome";
import { asRequestError } from "../../service";
import { DRAG_REGION, request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { agentStillOffered, useAgentToolActions } from "../settings/AgentTools";
import { AddButton, AddProjectFields, useAddProject } from "./AddProjectForm";
import { droppedFolder } from "./drops";
import { added, afterTools, STEP_ORDER } from "./logic";
import { useRowTerminals } from "./RowTerminal";
import { checklistStillOffered, ToolsList } from "./Tools";

/** The title bar: the OS's controls (the title bar screen's two ends), the product name and the steps. */
function TitleBar({ bar }: { bar: OnboardingTitleBar | undefined }) {
  const app = useRegistries();
  const Start = app.slots.get("title_bar_start")?.Component;
  const End = app.slots.get("title_bar_end")?.Component;
  return (
    <div className="onboarding-titlebar" {...DRAG_REGION}>
      {Start && <Start />}
      <span className="onboarding-product">{bar?.product}</span>
      <span className="onboarding-spacer" />
      <ol className="onboarding-steps">
        {bar?.steps.map((dot) => (
          <li key={dot.label} className="onboarding-step" data-state={dot.state}>
            <span className="onboarding-step-badge">
              {dot.state === "done" ? <Icon name="check" size={11} /> : dot.number}
            </span>
            <span className="onboarding-step-label">{dot.label}</span>
          </li>
        ))}
      </ol>
      <span className="onboarding-spacer" />
      {End && <End />}
    </div>
  );
}

function Card({ children, wide = false }: { children: ReactNode; wide?: boolean }) {
  return (
    <section className="onboarding-card" data-wide={wide || undefined}>
      {children}
    </section>
  );
}

/** One column of Welcome's picture: its name, who acts there, and the sample card when it has one. */
function PictureColumn({ column }: { column: OnboardingColumn }) {
  return (
    <div className="onboarding-model-column">
      <span className="onboarding-model-title">{column.title}</span>
      <span className="onboarding-model-cards">
        {column.card && (
          <span className="onboarding-model-card" data-needs-you={column.card.needs_you || undefined}>
            <span className="onboarding-model-key">{column.card.key}</span>
            <span className="onboarding-model-card-title">{column.card.title}</span>
            <span className="onboarding-model-status">{column.card.status}</span>
          </span>
        )}
      </span>
      <span className="onboarding-model-who">{column.who}</span>
    </div>
  );
}

function WelcomeStep({ welcome, onNext }: { welcome: OnboardingWelcome; onNext: () => void }) {
  return (
    <Card wide>
      <h1 className="onboarding-title onboarding-title-large">{welcome.title}</h1>
      <p className="onboarding-text onboarding-text-large">{welcome.text}</p>
      <div
        className="onboarding-model"
        role="img"
        aria-label={welcome.columns.map((c) => c.title).join(", ")}
      >
        {welcome.columns.map((column) => (
          <PictureColumn key={column.title} column={column} />
        ))}
      </div>
      <p className="onboarding-model-note">
        <span className="onboarding-model-dot" />
        <span>{welcome.needs_you}</span>
      </p>
      <div className="onboarding-foot">
        <span className="onboarding-foot-text">{welcome.foot}</span>
        <Button label={welcome.button} icon="arrow-right" onClick={onNext} />
      </div>
    </Card>
  );
}

function ProjectStep({ onAdded, drop }: { onAdded: (key: string) => void; drop: string | null }) {
  const state = useAddProject(true, drop, onAdded);
  const { view } = state;
  return (
    <Card>
      <h1 className="onboarding-title">{view?.title}</h1>
      {view?.text !== null && <p className="onboarding-text">{view?.text}</p>}
      <AddProjectFields state={state} />
      <div className="onboarding-foot">
        <span className="onboarding-foot-text">{view?.footnote}</span>
        <AddButton state={state} />
      </div>
    </Card>
  );
}

export function OnboardingOverlay({ params, close }: OverlayProps<"onboarding">) {
  const first = params?.step ?? "welcome";
  const [step, setStep] = useState<OnboardingStepId>(first);
  const [texts, setTexts] = useState<OnboardingTexts>();
  const [tools, setTools] = useState<OnboardingTools>();
  const [checking, setChecking] = useState(first === "tools");
  const [failure, setFailure] = useState<string | null>(null);
  const drop = useStore(droppedFolder);
  const focusRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    request("onboarding.texts").then(
      (answer) => {
        setTexts(answer as OnboardingTexts);
      },
      (error: unknown) => {
        setFailure(asRequestError(error).message);
      },
    );
  }, []);

  const runCheck = useCallback(
    () =>
      request("onboarding.tools").then(
        (answer) => {
          setTools(answer as OnboardingTools);
          setChecking(false);
        },
        (error: unknown) => {
          setChecking(false);
          setFailure(asRequestError(error).message);
        },
      ),
    [],
  );

  // An agent tool's row acts through `agent_tools.*`; the step then checks again (the page it answers is Settings').
  const ignorePage = useCallback(() => undefined, []);
  const recheckAfter = useCallback(() => {
    setChecking(true);
    void runCheck();
  }, [runCheck]);
  // One Install / Sign in… terminal for the agent tools and git; `tools.check` checks every tool again.
  const terminals = useRowTerminals(
    (key) =>
      agentStillOffered(tools?.agent_tools.rows ?? [], key) || checklistStillOffered(tools?.rows ?? [], key),
    () => {
      setChecking(true);
      return runCheck();
    },
  );
  const agentTools = useAgentToolActions(ignorePage, setFailure, recheckAfter, terminals);

  // Opened on the tools step (Check tools on the board): the check runs at once.
  useEffect(() => {
    if (first === "tools") void runCheck();
  }, [first, runCheck]);

  // Each step's first button takes the keyboard without scrolling to it: a step opens at its top (Tools' "Connect an
  // agent tool", not git's row at its foot).
  useEffect(() => {
    focusRef.current
      ?.querySelector<HTMLElement>(".onboarding-card .button[data-variant='primary']")
      ?.focus({ preventScroll: true });
  }, [step, texts, tools]);

  const check = () => {
    setStep("tools");
    setChecking(true);
    void runCheck();
  };

  const bar = texts?.title_bars[STEP_ORDER.indexOf(step)];
  let body: ReactNode = null;
  if (texts) {
    switch (step) {
      case "welcome":
        body = <WelcomeStep welcome={texts.welcome} onNext={check} />;
        break;
      case "tools":
        body = (
          <Card>
            {tools ? (
              <ToolsList
                tools={tools}
                checking={checking}
                onRecheck={() => {
                  setChecking(true);
                  void runCheck();
                }}
                onContinue={() => {
                  if (afterTools(tools) === "done") close();
                  else setStep("project");
                }}
                onAgentAction={agentTools.act}
                terminals={terminals}
              />
            ) : (
              <p className="onboarding-text">{texts.checking}</p>
            )}
          </Card>
        );
        break;
      case "project":
        body = (
          <ProjectStep
            key={drop?.seq ?? 0}
            drop={drop?.path ?? null}
            onAdded={(key) => {
              // The board, on Otaq default, with the first ticket in Backlog.
              added(key);
              close();
            }}
          />
        );
        break;
    }
  }

  return (
    <div
      className="onboarding"
      role="dialog"
      aria-modal="true"
      aria-label={bar?.product ?? ""}
      ref={focusRef}
    >
      <TitleBar bar={bar} />
      <div className="onboarding-body">
        {body}
        {failure !== null && <p className="onboarding-failure">{failure}</p>}
      </div>
      {agentTools.dialogs}
    </div>
  );
}
