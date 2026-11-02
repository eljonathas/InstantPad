import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect } from "react";
import { commands, type Message } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { whileHeld } from "../../shared/ui/hotkeyPause";
import { Knob } from "../../shared/ui/Knob";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import { AiSetup } from "./AiSetup";
import { isAiReady, type PendingAi } from "./ai";
import "./Voice.css";

interface VoiceControlsProps {
  pending: PendingAi | null;
  onPending: (pending: PendingAi | null) => void;
}

/** The settings of the effect in use, as knobs on the built-in screen. */
export function VoiceControls({ pending, onPending }: VoiceControlsProps) {
  const { t, render } = useText();
  const { active, activeCloneId, effects, isHearing, ai } = useBackend((state) => state.voice);
  const keys = useBackend((state) => state.setup.voiceKeys);
  const effect = effects.find((item) => item.effect === active && item.cloneId === activeCloneId);
  const isReady = isAiReady(ai);

  // The setup is done, so the voice the user asked for goes on now.
  useEffect(() => {
    if (!pending || !isReady) return;
    pending.apply();
    onPending(null);
  }, [pending, isReady, onPending]);

  async function deleteVoice(id: string, name: Message) {
    const question = t("DeleteVoiceQuestion", [render(name)]);
    const confirmed = await whileHeld(() => ask(question, { title: t("DeleteVoice"), kind: "warning" }));
    if (confirmed) await commands.deleteClonedVoice(id);
  }

  const cloneId = effect?.cloneId;
  const isSettingUp = pending !== null && !isReady;
  const title = isSettingUp ? pending.name : effect?.name;

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
      {!isSettingUp && effect?.needsAi && <p className="screen-text voice-note">{t("AiDelayNote")}</p>}
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
