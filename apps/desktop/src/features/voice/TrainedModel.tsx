import { ask, open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { commands, type EffectView } from "../../shared/ipc";
import { useText } from "../../shared/store/text";
import { whileHeld } from "../../shared/ui/hotkeyPause";
import { ToggleKey } from "../../shared/ui/ToggleKey";

/**
 * The trained model of an AI voice: the engine the voice speaks through, and the keys that add,
 * replace and remove the model the training kit exported.
 */
export function TrainedModel({ effect }: { effect: EffectView }) {
  const { t } = useText();
  const [isAdding, setAdding] = useState(false);
  const { trained } = effect;
  const voice = [effect.effect, effect.cloneId] as const;

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
      {!trained && <p className="screen-text voice-note">{t("TrainedModelHint")}</p>}
      <div className="trained-keys">
        <button type="button" className="key trained-key" disabled={isAdding} onClick={() => void add()}>
          {t(isAdding ? "AddingTrainedModel" : trained ? "ReplaceTrainedModel" : "AddTrainedModel")}
        </button>
        {trained && (
          <button type="button" className="key trained-key" onClick={() => void remove(trained.name)}>
            {t("RemoveTrainedModel")}
          </button>
        )}
      </div>
    </div>
  );
}
