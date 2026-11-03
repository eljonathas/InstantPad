import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, type ReactNode } from "react";
import { pickAudioFiles } from "../../shared/audioFiles";
import type { TextId } from "../../shared/i18n";
import { commands, type Message, type TrainingDevice, type TrainStep } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { whileHeld } from "../../shared/ui/hotkeyPause";
import { Dialog } from "../../shared/ui/Dialog";
import { RecordKey } from "./RecordKey";
import { clock, duration } from "./trainingTime";
import "./Clone.css";
import "./Train.css";

type Step<Name extends TrainStep["step"]> = Extract<TrainStep, { step: Name }>;

const STAGES: Record<Step<"training">["stage"], TextId> = {
  cutting: "TrainingStageCutting",
  pitch: "TrainingStagePitch",
  words: "TrainingStageWords",
  learning: "TrainingStageLearning",
  exporting: "TrainingStageExporting",
};

function useDuration() {
  const { t } = useText();
  return (seconds: number) => t(...duration(seconds));
}

function Problem({ message }: { message: Message | null }) {
  const { render } = useText();
  return message && <p className="clone-problem">{render(message)}</p>;
}

function Buttons({ children }: { children?: ReactNode }) {
  const { t } = useText();
  return (
    <div className="dialog-buttons train-buttons">
      <button type="button" className="key" onClick={() => void commands.closeTraining()}>
        {t("Close")}
      </button>
      {children}
    </div>
  );
}

function DiscardKey() {
  const { t } = useText();

  async function discard() {
    const confirmed = await whileHeld(() =>
      ask(t("DiscardTrainingQuestion"), { title: t("DiscardTraining"), kind: "warning" }),
    );
    if (confirmed) commands.discardTraining();
  }

  return (
    <button type="button" className="key" onClick={() => void discard()}>
      {t("DiscardTraining")}
    </button>
  );
}

/** Where the voice trains: on the card, or on the processor and why, which takes far longer. */
function Device({ device, card, seconds }: { device: TrainingDevice; card: string | null; seconds: number }) {
  const { t } = useText();
  const time = useDuration()(seconds);
  if (device.kind === "card") return <p className="clone-text">{t("TrainOnCard", [card ?? "", time])}</p>;
  const why: Record<typeof device.reason, TextId> = {
    noCard: "TrainNoCard",
    smallCard: "TrainSmallCard",
    oldDriver: "TrainOldDriver",
  };
  return (
    <p className="clone-text train-warning">
      {t(why[device.reason], [card ?? ""])} {t("TrainOnProcessor", [time])}
    </p>
  );
}

function Prepare({ step }: { step: Step<"prepare"> }) {
  const { t } = useText();
  const isDownloaded = step.downloadMegabytes === 0;
  // A training already begun, whose download was stopped or whose tools were removed since, can
  // still be given up here, with its recordings. Only one voice trains at a time, so it is this one.
  const hasTraining = useBackend((state) => state.voice.training.job !== null);

  return (
    <>
      <p className="clone-text">{t("TrainIntro", [step.voice])}</p>
      <Device device={step.device} card={step.card} seconds={step.estimatedSeconds} />
      <p className="clone-text">
        {isDownloaded
          ? t("TrainToolsReady", [String(step.neededMegabytes)])
          : t("TrainDownload", [String(step.downloadMegabytes), String(step.neededMegabytes)])}
      </p>
      {step.missingMegabytes !== null && (
        <p className="clone-problem">{t("TrainMissingRoom", [String(step.missingMegabytes)])}</p>
      )}
      <p className="clone-text train-note">{t("TrainRecordingsNote")}</p>
      <p className="clone-text train-note">{t("TrainConsent")}</p>
      <Problem message={step.problem} />
      <Buttons>
        {hasTraining && <DiscardKey />}
        <button
          type="button"
          className="key"
          disabled={step.missingMegabytes !== null}
          onClick={() => void commands.acceptTraining()}
        >
          {t(isDownloaded ? "Continue" : "TrainDownloadKey")}
        </button>
      </Buttons>
    </>
  );
}

function Installing({ step }: { step: Step<"installing"> }) {
  const { t } = useText();

  return (
    <>
      <p className="clone-text">{t("TrainInstalling", [String(step.percent)])}</p>
      <progress className="ai-progress train-progress" max={100} value={step.percent} />
      <p className="clone-text train-note">{t("TrainInBackground")}</p>
      <Buttons>
        <button type="button" className="key" onClick={() => commands.stopTraining()}>
          {t("StopDownload")}
        </button>
      </Buttons>
    </>
  );
}

function Takes({ step }: { step: Step<"takes"> }) {
  const { t } = useText();
  const time = useDuration();
  const minutes = (seconds: number) => String(Math.round(seconds / 60));
  const isFull = step.seconds >= step.maxSeconds;
  const canTrain = step.seconds >= step.minSeconds && !step.isRecording && !step.isBusy;

  // A take ends by itself at its longest, so a forgotten recording does not fill the training.
  useEffect(() => {
    if (!step.isRecording) return;
    const room = Math.min(step.maxTakeSeconds, step.maxSeconds - step.seconds);
    const timer = setTimeout(() => void commands.finishTrainingTake(), room * 1000);
    return () => clearTimeout(timer);
  }, [step.isRecording, step.maxTakeSeconds, step.maxSeconds, step.seconds]);

  async function chooseFiles() {
    for (const path of await pickAudioFiles(t, true)) await commands.importTrainingTake(path);
  }

  return (
    <>
      <p className="clone-text">
        {t("TrainTakesIntro", [minutes(step.minSeconds), minutes(step.maxSeconds), minutes(step.maxTakeSeconds)])}
      </p>
      <Problem message={step.problem} />
      <div className="clone-actions">
        <RecordKey
          isRecording={step.isRecording}
          onStart={() => commands.startTrainingTake()}
          onStop={() => void commands.finishTrainingTake()}
        />
        {!step.isRecording && (
          <button type="button" className="key" disabled={step.isBusy || isFull} onClick={() => void chooseFiles()}>
            {t("ChooseFiles")}
          </button>
        )}
      </div>
      <p className="clone-text train-total">
        {t("TrainSpeech", [clock(step.seconds), clock(step.minSeconds), clock(step.maxSeconds)])}
      </p>
      <progress className="ai-progress train-progress" max={step.maxSeconds} value={step.seconds} />
      {step.takes.length > 0 && (
        <ol className="train-takes">
          {step.takes.map((take, index) => (
            <li key={take.id} className="train-take">
              <span>{t("TrainTake", [String(index + 1), clock(take.seconds)])}</span>
              <button
                type="button"
                className="key train-take-key"
                disabled={step.isRecording || step.isBusy}
                onClick={() => commands.removeTrainingTake(take.id)}
              >
                {t("Remove")}
              </button>
            </li>
          ))}
        </ol>
      )}
      {step.seconds > 0 && <p className="clone-text train-note">{t("TrainEstimate", [time(step.estimatedSeconds)])}</p>}
      <Buttons>
        <DiscardKey />
        <button type="button" className="key" disabled={!canTrain} onClick={() => void commands.trainVoice()}>
          {t("StartTraining")}
        </button>
      </Buttons>
    </>
  );
}

function Training({ step }: { step: Step<"training"> }) {
  const { t } = useText();
  const time = useDuration();
  const isLearning = step.stage === "learning";

  return (
    <>
      <p className="clone-text">
        {step.isRunning
          ? t(STAGES[step.stage], [String(step.epoch), String(step.epochs)])
          : t("TrainingPausedAt", [String(step.epoch), String(step.epochs)])}
      </p>
      <progress className="ai-progress train-progress" max={step.epochs} value={step.epoch} />
      {step.isRunning && isLearning && step.remainingSeconds !== null && (
        <p className="clone-text">{t("TrainRemaining", [time(step.remainingSeconds)])}</p>
      )}
      {step.device.kind === "processor" && <p className="clone-text train-warning">{t("TrainSlowNote")}</p>}
      <p className="clone-text train-note">{t(step.isRunning ? "TrainInBackground" : "TrainResumeNote")}</p>
      <Problem message={step.problem} />
      <Buttons>
        {step.isRunning ? (
          <button type="button" className="key" onClick={() => commands.stopTraining()}>
            {t("PauseTraining")}
          </button>
        ) : (
          <>
            <DiscardKey />
            <button type="button" className="key" onClick={() => void commands.trainVoice()}>
              {t("ResumeTraining")}
            </button>
          </>
        )}
      </Buttons>
    </>
  );
}

/** The steps of training a model of a cloned voice. The backend says which one is open. */
export function TrainDialog() {
  const { t } = useText();
  const step = useBackend((state) => state.training);
  if (step.step === "closed") return null;

  return (
    <Dialog title={t("TrainModel")} isWide onCancel={() => void commands.closeTraining()}>
      {step.step === "checking" && (
        <>
          <p className="clone-text">{t("TrainChecking")}</p>
          <Buttons />
        </>
      )}
      {step.step === "prepare" && <Prepare step={step} />}
      {step.step === "installing" && <Installing step={step} />}
      {step.step === "takes" && <Takes step={step} />}
      {step.step === "training" && <Training step={step} />}
    </Dialog>
  );
}
