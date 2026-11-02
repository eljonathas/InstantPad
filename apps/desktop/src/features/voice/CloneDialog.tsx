import { useState, type ReactNode } from "react";
import { pickAudioFiles } from "../../shared/audioFiles";
import { commands, type CloneStep, type Message } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Dialog, DialogLabel } from "../../shared/ui/Dialog";
import { RecordKey } from "./RecordKey";
import "./Clone.css";

const SAMPLE_TIPS = ["CloneTip1", "CloneTip2", "CloneTip3", "CloneTip4", "CloneTip5", "CloneTip6"] as const;

type Step<Name extends CloneStep["step"]> = Extract<CloneStep, { step: Name }>;

function Problem({ message }: { message: Message | null }) {
  const { render } = useText();
  return message && <p className="clone-problem">{render(message)}</p>;
}

function Buttons({ children }: { children?: ReactNode }) {
  const { t } = useText();
  return (
    <div className="dialog-buttons">
      <button type="button" className="key" onClick={() => void commands.closeCloning()}>
        {t("Cancel")}
      </button>
      {children}
    </div>
  );
}

function Sample({ step }: { step: Step<"sample"> }) {
  const { t } = useText();

  async function chooseFile() {
    const [path] = await pickAudioFiles(t, false);
    if (path) await commands.importCloneSample(path);
  }

  return (
    <>
      <p className="clone-text">{t("CloneSampleIntro", [String(step.minSeconds), String(step.maxSeconds)])}</p>
      <Problem message={step.problem} />
      <div className="clone-actions">
        <RecordKey
          isRecording={step.isRecording}
          onStart={() => void commands.startCloneSample()}
          onStop={() => void commands.finishCloneSample()}
        />
        {!step.isRecording && (
          <button type="button" className="key" onClick={() => void chooseFile()}>
            {t("ChooseFile")}
          </button>
        )}
      </div>
      <h3 className="clone-tips-title">{t("CloneTipsTitle")}</h3>
      <ol className="clone-tips">
        {SAMPLE_TIPS.map((tip) => (
          <li key={tip}>{t(tip)}</li>
        ))}
      </ol>
      <Buttons />
    </>
  );
}

function Analyzing() {
  const { t } = useText();

  return (
    <>
      <p className="clone-text">{t("CloneAnalyzing")}</p>
      <Buttons />
    </>
  );
}

function NameVoice() {
  const { t } = useText();
  const [name, setName] = useState("");
  const save = () => void commands.saveClonedVoice(name.trim());

  return (
    <>
      <p className="clone-text">{t("CloneNameIntro")}</p>
      <DialogLabel>{t("Name")}</DialogLabel>
      <input
        className="field dialog-field"
        aria-label={t("Name")}
        value={name}
        autoFocus
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && name.trim() && save()}
      />
      <Buttons>
        <button type="button" className="key" disabled={name.trim().length === 0} onClick={save}>
          {t("Save")}
        </button>
      </Buttons>
    </>
  );
}

/** The steps of cloning a voice. The backend says which one is open and what went wrong. */
export function CloneDialog() {
  const { t } = useText();
  const step = useBackend((state) => state.cloning);
  if (step.step === "closed") return null;

  return (
    <Dialog title={t("CloneVoice")} isWide onCancel={() => void commands.closeCloning()}>
      {step.step === "sample" && <Sample step={step} />}
      {step.step === "analyzing" && <Analyzing />}
      {step.step === "name" && <NameVoice />}
    </Dialog>
  );
}
