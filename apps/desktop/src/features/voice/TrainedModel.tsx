import { ask, open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { commands, type EffectView, type TrainingPhase } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText, type Translate } from "../../shared/store/text";
import { whileHeld } from "../../shared/ui/hotkeyPause";
import { ToggleKey } from "../../shared/ui/ToggleKey";

/** How far the training of this voice got, for the voice screen while the dialog is closed. */
function phaseText(t: Translate, phase: TrainingPhase): string {
  switch (phase.kind) {
    case "takes":
      return t("TrainingPhaseTakes", [String(Math.floor(phase.seconds / 60)), String(phase.minSeconds / 60)]);
    case "installing":
      return t("TrainingPhaseInstalling", [String(phase.percent)]);
    case "running":
      return t("TrainingPhaseRunning", [String(phase.epoch), String(phase.epochs)]);
    case "paused":
      return t("TrainingPausedAt", [String(phase.epoch), String(phase.epochs)]);
  }
}

/**
 * The trained model of an AI voice: the engine the voice speaks through, and the keys that add,
 * replace and remove the model the training kit exported. A cloned voice also trains its model
 * here, on systems that can.
 */
export function TrainedModel({ effect }: { effect: EffectView }) {
  const { t } = useText();
  const [isAdding, setAdding] = useState(false);
  const { trained } = effect;
  const voice = [effect.effect, effect.cloneId] as const;
  const training = useBackend((state) => state.voice.training);
  const cloneId = effect.cloneId;
  const canTrain = training.isSupported && cloneId !== null;
  const job = training.job?.cloneId === cloneId ? training.job : null;
  const isTraining = training.job?.phase.kind === "running" || training.job?.phase.kind === "installing";

  async function add() {
    const folder = await whileHeld(() => open({ directory: true, title: t("AddTrainedModel") }));
    if (typeof folder !== "string") return;
    setAdding(true);
    try {
      await commands.addTrainedModel(...voice, folder);
    } finally {
      setAdding(false);
    }
  }

  async function remove(name: string) {
    const question = t("RemoveTrainedModelQuestion", [name]);
    const confirmed = await whileHeld(() => ask(question, { title: t("RemoveTrainedModel"), kind: "warning" }));
    if (confirmed) await commands.removeTrainedModel(...voice);
  }

  async function removeTools() {
    const question = t("RemoveTrainingToolsQuestion", [String(training.toolsMegabytes)]);
    const confirmed = await whileHeld(() => ask(question, { title: t("RemoveTrainingTools"), kind: "warning" }));
    if (confirmed) commands.removeTrainingTools();
  }

  return (
    <div className="trained">
      {trained && (
        <>
          <div className="trained-engines" role="group" aria-label={t("VoiceEngine")}>
            <span className="silk trained-label">{t("VoiceEngine")}</span>
            <ToggleKey
              kind="switch"
              className="trained-engine"
              label={t("EngineInstant")}
              on={trained.engine === "instant"}
              onClick={() => void commands.setVoiceEngine(...voice, "instant")}
            />
            <ToggleKey
              kind="switch"
              className="trained-engine"
              label={t("EngineTrained")}
              on={trained.engine === "trained"}
              onClick={() => void commands.setVoiceEngine(...voice, "trained")}
            />
          </div>
          {trained.engine === "trained" && (
            <p className="screen-text voice-note">{t("TrainedEngineNote", [trained.name])}</p>
          )}
        </>
      )}
      {!trained && !job && (
        <p className="screen-text voice-note">{t(canTrain ? "TrainedModelTrainHint" : "TrainedModelHint")}</p>
      )}
      {job && <p className="screen-text voice-note">{phaseText(t, job.phase)}</p>}
      <div className="trained-keys">
        {canTrain && (
          <button type="button" className="key trained-key" onClick={() => void commands.openTraining(cloneId)}>
            {t(job ? "OpenTraining" : "TrainModelKey")}
          </button>
        )}
        <button type="button" className="key trained-key" disabled={isAdding} onClick={() => void add()}>
          {t(isAdding ? "AddingTrainedModel" : trained ? "ReplaceTrainedModel" : "AddTrainedModel")}
        </button>
        {trained && (
          <button type="button" className="key trained-key" onClick={() => void remove(trained.name)}>
            {t("RemoveTrainedModel")}
          </button>
        )}
        {canTrain && training.toolsMegabytes > 0 && !isTraining && (
          <button type="button" className="key trained-key" onClick={() => void removeTools()}>
            {t("RemoveTrainingTools")}
          </button>
        )}
      </div>
    </div>
  );
}
