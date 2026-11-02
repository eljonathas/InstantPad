import { useEffect, useRef, useState } from "react";
import { commands, type DesignerView, type VoiceRecipe } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Dialog, DialogLabel } from "../../shared/ui/Dialog";
import { Knob } from "../../shared/ui/Knob";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import "./Design.css";

// A knob turns many times a second, and each preview change reaches the audio engine.
const PREVIEW_DELAY_MS = 150;

/** How far the timbre is from where it starts, as a signed percent of the way to either end. */
const timbreText = (timbre: number, start: number) => {
  const percent = Math.round((timbre - start) * 200);
  return `${percent > 0 ? "+" : ""}${percent}%`;
};

/** Makes a new voice out of a cloned one: shifts its timbre and mixes another cloned voice into it. */
function Designer({ designer }: { designer: DesignerView }) {
  const { t } = useText();
  const voices = useBackend((state) => state.voice.clonedVoices);
  const [recipe, setRecipe] = useState<VoiceRecipe>({
    baseId: voices[0].id,
    timbre: designer.defaultTimbre,
    blendId: null,
    blend: designer.defaultBlend,
  });
  const [name, setName] = useState("");
  const change = (patch: Partial<VoiceRecipe>) => setRecipe({ ...recipe, ...patch });
  const { isPreviewing } = designer;

  // The recipe the engine last got, so turning the preview on does not send it twice.
  const sent = useRef<VoiceRecipe | null>(null);
  const send = (next: VoiceRecipe) => {
    sent.current = next;
    void commands.previewDesignedVoice(next);
  };

  useEffect(() => {
    if (!isPreviewing || sent.current === recipe) return;
    const timer = setTimeout(() => send(recipe), PREVIEW_DELAY_MS);
    return () => clearTimeout(timer);
  }, [isPreviewing, recipe]);

  const save = () => void commands.saveDesignedVoice(name.trim(), recipe);
  const others = voices.filter((voice) => voice.id !== recipe.baseId);

  return (
    <Dialog title={t("DesignVoice")} isWide onCancel={() => void commands.closeDesigner()}>
      <DialogLabel>{t("DesignBase")}</DialogLabel>
      <select
        className="field dialog-field"
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
            value={recipe.timbre}
            defaultValue={designer.defaultTimbre}
            text={timbreText(recipe.timbre, designer.defaultTimbre)}
            onChange={(timbre) => change({ timbre })}
          />
          <div className="silk">{t("DesignTimbre")}</div>
          <div className="design-ends">
            <span>{t("TimbreDeeper")}</span>
            <span>{timbreText(recipe.timbre, designer.defaultTimbre)}</span>
            <span>{t("TimbreHigher")}</span>
          </div>
        </div>

        <div className="design-control">
          <DialogLabel>{t("DesignMixWith")}</DialogLabel>
          <select
            className="field dialog-field"
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
                defaultValue={designer.defaultBlend}
                onChange={(blend) => change({ blend })}
              />
              <div className="silk">{t("DesignAmount")}</div>
              <div className="design-ends">{Math.round(recipe.blend * 100)}%</div>
            </>
          )}
        </div>
      </div>

      <p className="dialog-tip">{t("DesignTip")}</p>

      <ToggleKey
        kind="switch"
        className="design-preview"
        label={t("DesignHearPreview")}
        on={isPreviewing}
        onClick={() => (isPreviewing ? void commands.endVoicePreview() : send(recipe))}
      />

      <DialogLabel>{t("Name")}</DialogLabel>
      <input
        className="field dialog-field"
        aria-label={t("Name")}
        value={name}
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && name.trim() && save()}
      />

      <div className="dialog-buttons">
        <button type="button" className="key" onClick={() => void commands.closeDesigner()}>
          {t("Cancel")}
        </button>
        <button
          type="button"
          className="key"
          disabled={name.trim().length === 0}
          onClick={save}
        >
          {t("Save")}
        </button>
      </div>
    </Dialog>
  );
}

/** The designer follows the backend: it is open while the voice view says so. */
export function DesignDialog() {
  const designer = useBackend((state) => state.voice.designer);
  return designer && <Designer designer={designer} />;
}
