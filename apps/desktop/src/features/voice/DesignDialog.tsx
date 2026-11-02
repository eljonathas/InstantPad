import { useEffect, useState } from "react";
import { commands, type VoiceRecipe } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Dialog, DialogLabel } from "../../shared/ui/Dialog";
import { Knob } from "../../shared/ui/Knob";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import "./Design.css";

// A knob turns many times a second, and each preview change reaches the audio engine.
const PREVIEW_DELAY_MS = 150;
const DEFAULT_BLEND = 0.3;

const fromKnob = (value: number) => value * 2 - 1;
const toKnob = (timbre: number) => (timbre + 1) / 2;

/** The timbre as the user reads it: a signed percent, with 0% meaning the voice is unchanged. */
const timbreText = (timbre: number) => {
  const percent = Math.round(timbre * 100);
  return `${percent > 0 ? "+" : ""}${percent}%`;
};

/** Makes a new voice out of a cloned one: shifts its timbre and mixes another cloned voice into it. */
export function DesignDialog({ onDone }: { onDone: () => void }) {
  const { t } = useText();
  const voices = useBackend((state) => state.voice.clonedVoices);
  const [recipe, setRecipe] = useState<VoiceRecipe>({
    baseId: voices[0].id,
    timbre: 0,
    blendId: null,
    blend: DEFAULT_BLEND,
  });
  const [isPreviewing, setIsPreviewing] = useState(false);
  const [name, setName] = useState("");
  const change = (patch: Partial<VoiceRecipe>) => setRecipe({ ...recipe, ...patch });

  useEffect(() => {
    if (!isPreviewing) return;
    const timer = setTimeout(() => void commands.previewDesignedVoice(recipe), PREVIEW_DELAY_MS);
    return () => clearTimeout(timer);
  }, [isPreviewing, recipe]);

  // Whatever way the dialog closes, the user goes back to hearing the effect in use.
  useEffect(() => () => void commands.endVoicePreview(), []);

  function togglePreview() {
    if (isPreviewing) void commands.endVoicePreview();
    else void commands.setHearingVoice(true);
    setIsPreviewing(!isPreviewing);
  }

  async function save() {
    await commands.saveDesignedVoice(name.trim(), recipe);
    onDone();
  }

  const others = voices.filter((voice) => voice.id !== recipe.baseId);

  return (
    <Dialog title={t("DesignVoice")} isWide onCancel={onDone}>
      <DialogLabel>{t("DesignBase")}</DialogLabel>
      <select
        className="field design-field"
        aria-label={t("DesignBase")}
        value={recipe.baseId}
        onChange={(event) =>
          change({
            baseId: event.target.value,
            blendId: recipe.blendId === event.target.value ? null : recipe.blendId,
          })
        }
      >
        {voices.map((voice) => (
          <option key={voice.id} value={voice.id}>
            {voice.name}
          </option>
        ))}
      </select>

      <div className="design-controls">
        <div className="design-control">
          <Knob
            label={t("DesignTimbre")}
            value={toKnob(recipe.timbre)}
            defaultValue={toKnob(0)}
            text={timbreText(recipe.timbre)}
            onChange={(value) => change({ timbre: fromKnob(value) })}
          />
          <div className="silk">{t("DesignTimbre")}</div>
          <div className="design-ends">
            <span>{t("TimbreDeeper")}</span>
            <span>{timbreText(recipe.timbre)}</span>
            <span>{t("TimbreHigher")}</span>
          </div>
        </div>

        <div className="design-control">
          <DialogLabel>{t("DesignMixWith")}</DialogLabel>
          <select
            className="field design-field"
            aria-label={t("DesignMixWith")}
            value={recipe.blendId ?? ""}
            onChange={(event) => change({ blendId: event.target.value || null })}
          >
            <option value="">{t("DesignMixNone")}</option>
            {others.map((voice) => (
              <option key={voice.id} value={voice.id}>
                {voice.name}
              </option>
            ))}
          </select>
          {recipe.blendId && (
            <>
              <Knob
                label={t("DesignAmount")}
                value={recipe.blend}
                defaultValue={DEFAULT_BLEND}
                onChange={(blend) => change({ blend })}
              />
              <div className="silk">{t("DesignAmount")}</div>
              <div className="design-ends">{Math.round(recipe.blend * 100)}%</div>
            </>
          )}
        </div>
      </div>

      <p className="design-tip">{t("DesignTip")}</p>

      <ToggleKey
        kind="switch"
        className="design-preview"
        label={t("DesignHearPreview")}
        on={isPreviewing}
        onClick={togglePreview}
      />

      <DialogLabel>{t("Name")}</DialogLabel>
      <input
        className="field design-field"
        aria-label={t("Name")}
        value={name}
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && name.trim() && void save()}
      />

      <div className="dialog-buttons">
        <button type="button" className="key" onClick={onDone}>
          {t("Cancel")}
        </button>
        <button type="button" className="key" disabled={name.trim().length === 0} onClick={() => void save()}>
          {t("Save")}
        </button>
      </div>
    </Dialog>
  );
}
