import { useState } from "react";
import { useText } from "../../shared/store/text";
import { Knob } from "../../shared/ui/Knob";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import type { Effect } from "./effects";
import "./Voice.css";

/** The settings of the effect picked on the board, as knobs on the built-in screen. */
export function VoiceControls({ effect }: { effect: Effect }) {
  const { t } = useText();
  const [values, setValues] = useState<Record<string, number>>({});
  const [hearing, setHearing] = useState(false);

  return (
    <div className="voice-controls">
      <h3 className="screen-text voice-title">{t(effect.name)}</h3>
      <div className="voice-knobs">
        {effect.params.length === 0 && <p className="screen-text voice-empty">{t("EffectNaturalHint")}</p>}
        {effect.params.map((param) => {
          const key = `${effect.id}.${param.id}`;
          return (
            <div key={key}>
              <Knob
                label={t(param.label)}
                value={values[key] ?? param.value}
                defaultValue={param.value}
                onChange={(value) => setValues({ ...values, [key]: value })}
              />
              <div className="silk">{t(param.label)}</div>
            </div>
          );
        })}
      </div>
      <ToggleKey
        kind="switch"
        className="voice-hear"
        label={t("HearMyVoice")}
        on={hearing}
        onClick={() => setHearing(!hearing)}
      />
      <p className="screen-text voice-note">{t("HearMyVoiceHint")}</p>
    </div>
  );
}
