import { ask } from "@tauri-apps/plugin-dialog";
import { commands, type Message } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { whileHeld } from "../../shared/ui/hotkeyPause";
import { Knob } from "../../shared/ui/Knob";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import { AiSetup } from "./AiSetup";
import { TrainedModel } from "./TrainedModel";
import "./Voice.css";

/** The settings of the effect in use, as knobs on the built-in screen. */
export function VoiceControls() {
  const { t, render } = useText();
  const { active, activeCloneId, effects, isHearing, ai, clonedVoices } = useBackend((state) => state.voice);
  const keys = useBackend((state) => state.setup.voiceKeys);
  const effect = effects.find((item) => item.effect === active && item.cloneId === activeCloneId);

  async function deleteVoice(id: string, name: Message) {
    // Deleting a cloned voice takes the voices designed from it along, which the question says.
    const isCloned = clonedVoices.some((voice) => voice.id === id);
    const question = t(isCloned ? "DeleteClonedVoiceQuestion" : "DeleteVoiceQuestion", [render(name)]);
    const confirmed = await whileHeld(() => ask(question, { title: t("DeleteVoice"), kind: "warning" }));
    if (confirmed) await commands.deleteClonedVoice(id);
  }

  const cloneId = effect?.cloneId;
  // Whatever the AI still needs, or the calibration that is running, takes the panel until it is done.
  const isSettingUp = ai.waiting !== null || ai.calibration !== "idle";
  const title = ai.waiting ?? effect?.name;

  return (
    <div className="voice-controls">
      {title && <h3 className="screen-text voice-title">{render(title)}</h3>}
      {isSettingUp ? (
        <div className="voice-setup">
          <AiSetup ai={ai} />
        </div>
      ) : (
        <div className="voice-knobs">
          {effect?.params.length === 0 && <p className="screen-text voice-empty">{t("EffectNaturalHint")}</p>}
          {effect?.params.map(({ param, label, value, defaultValue, text }) => (
            <div key={param}>
              <Knob
                label={t(label)}
                value={value}
                defaultValue={defaultValue}
                text={text}
                onChange={(next) => void commands.setVoiceParam(param, next)}
              />
              <div className="silk">{t(label)}</div>
              <div className="screen-text voice-value">{text}</div>
            </div>
          ))}
        </div>
      )}
      {!isSettingUp && effect?.takesTrainedModel && <TrainedModel effect={effect} />}
      {!isSettingUp && effect?.needsAi && effect.trained?.engine !== "trained" && (
        <p className="screen-text voice-note">{t("AiDelayNote")}</p>
      )}
      {!isSettingUp && cloneId && <p className="screen-text voice-note">{t("CloneUsageTip")}</p>}
      {/* A trained model needs no knowledge of the user's voice. */}
      {!isSettingUp && effect?.needsAi && effect.trained?.engine !== "trained" && (
        <button type="button" className="key voice-relearn" onClick={() => void commands.relearnVoice()}>
          {t("AiRelearn")}
        </button>
      )}
      {!isSettingUp && effect && cloneId && (
        <button
          type="button"
          className="key key-danger voice-delete"
          onClick={() => void deleteVoice(cloneId, effect.name)}
        >
          {t("DeleteVoice")}
        </button>
      )}
      {keys.length > 0 && <p className="screen-text voice-note">{t("VoiceKeysHint", keys)}</p>}
      <ToggleKey
        kind="switch"
        className="voice-hear"
        label={t("HearMyVoice")}
        on={isHearing}
        onClick={() => void commands.setHearingVoice(!isHearing)}
      />
      <p className="screen-text voice-note">{t("HearMyVoiceHint")}</p>
    </div>
  );
}
