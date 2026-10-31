import { commands } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Knob } from "../../shared/ui/Knob";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import "./Voice.css";

/** The settings of the effect in use, as knobs on the built-in screen. */
export function VoiceControls() {
  const { t } = useText();
  const { active, effects, isHearing } = useBackend((state) => state.voice);
  const keys = useBackend((state) => state.setup.voiceKeys);
  const effect = effects.find((item) => item.effect === active);

  return (
    <div className="voice-controls">
      {effect && <h3 className="screen-text voice-title">{t(effect.name)}</h3>}
      <div className="voice-knobs">
        {effect?.params.length === 0 && <p className="screen-text voice-empty">{t("EffectNaturalHint")}</p>}
        {effect?.params.map(({ param, label, value, defaultValue, text }) => (
          <div key={param}>
            <Knob
              label={t(label)}
              value={value}
              defaultValue={defaultValue}
              onChange={(next) => void commands.setVoiceParam(param, next)}
            />
            <div className="silk">{t(label)}</div>
            <div className="screen-text voice-value">{text}</div>
          </div>
        ))}
      </div>
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
