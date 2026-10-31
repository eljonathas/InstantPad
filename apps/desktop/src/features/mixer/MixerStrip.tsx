import { commands, type MixerView } from "../../shared/ipc";
import { setMixer, useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Knob } from "../../shared/ui/Knob";
import { LedMeter } from "../../shared/ui/LedMeter";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import "./MixerStrip.css";

type Volume = "micVolume" | "gameVolume" | "monitorVolume";

const knobs = [
  { field: "micVolume", text: "Mic", send: commands.setMicVolume },
  { field: "gameVolume", text: "Game", send: commands.setGameVolume },
  { field: "monitorVolume", text: "Headphones", send: commands.setMonitorVolume },
] as const satisfies readonly {
  field: Volume;
  text: "Mic" | "Game" | "Headphones";
  send: (volume: number) => Promise<void>;
}[];

/** The knobs, the two LED meters and the mic switch of the left strip. */
export function MixerStrip() {
  const { t } = useText();
  const mixer = useBackend((state) => state.mixer);
  const defaults = useBackend((state) => state.limits.defaultMixer);

  function change(field: Volume, send: (volume: number) => Promise<void>, volume: number) {
    setMixer({ [field]: volume } satisfies Partial<MixerView>);
    void send(volume);
  }

  return (
    <div className="mixer-strip">
      <div className="knobs">
        {knobs.map(({ field, text, send }) => (
          <div key={field}>
            <Knob
              label={t(text)}
              value={mixer[field]}
              defaultValue={defaults[field]}
              onChange={(volume) => change(field, send, volume)}
            />
            <div className="silk">{t(text)}</div>
          </div>
        ))}
      </div>

      <div className="meters">
        <div className="meter-cell">
          <LedMeter channel="micLevel" label={t("Mic")} />
          <div className="silk meter-label">{t("Mic")}</div>
        </div>
        <div className="meter-cell">
          <LedMeter channel="outputLevel" label={t("Output")} />
          <div className="silk meter-label">{t("Output")}</div>
        </div>
      </div>

      <ToggleKey
        kind="switch"
        className="mic-switch"
        label={mixer.micEnabled ? t("MicOpen") : t("MicMuted")}
        on={mixer.micEnabled}
        onClick={() => {
          setMixer({ micEnabled: !mixer.micEnabled });
          void commands.setMicEnabled(!mixer.micEnabled);
        }}
      />
    </div>
  );
}
