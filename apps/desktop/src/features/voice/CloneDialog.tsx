import { open } from "@tauri-apps/plugin-dialog";
import { useState, type ReactNode } from "react";
import { AUDIO_EXTENSIONS } from "../../shared/audioExtensions";
import { commands, type CloneStep, type Message, type TextKey } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Dialog, DialogLabel } from "../../shared/ui/Dialog";
import { whileHeld } from "../../shared/ui/hotkeyPause";
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

function Terms({ clauses }: { clauses: TextKey[] }) {
  const { t } = useText();
  const [accepted, setAccepted] = useState<TextKey[]>([]);
  const [ownerName, setOwnerName] = useState("");
  const toggle = (clause: TextKey) =>
    setAccepted((now) => (now.includes(clause) ? now.filter((item) => item !== clause) : [...now, clause]));
  const name = ownerName.trim();
  const isComplete = clauses.every((clause) => accepted.includes(clause)) && name.length > 0;

  return (
    <>
      <p className="clone-text">{t("CloneTermsIntro")}</p>
      <ul className="clone-clauses">
        {clauses.map((clause) => (
          <li key={clause}>
            <label className="clone-clause">
              <input type="checkbox" checked={accepted.includes(clause)} onChange={() => toggle(clause)} />
              <span>{t(clause)}</span>
            </label>
          </li>
        ))}
      </ul>
      <DialogLabel>{t("CloneOwnerName")}</DialogLabel>
      <input
        className="field dialog-field"
        aria-label={t("CloneOwnerName")}
        value={ownerName}
        onChange={(event) => setOwnerName(event.target.value)}
      />
      <Buttons>
        <button
          type="button"
          className="key"
          disabled={!isComplete}
          onClick={() => void commands.acceptCloneTerms(name, accepted)}
        >
          {t("Continue")}
        </button>
      </Buttons>
    </>
  );
}

function Sample({ step }: { step: Step<"sample"> }) {
  const { t } = useText();

  async function chooseFile() {
    const filters = [{ name: t("AudioFileType"), extensions: AUDIO_EXTENSIONS }];
    const path = await whileHeld(() => open({ filters }));
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

function Phrase({ step }: { step: Step<"phrase"> }) {
  const { t } = useText();

  return (
    <>
      <p className="clone-text">{t("ClonePhraseIntro", [String(step.maxSeconds)])}</p>
      <p className="clone-digits">{step.digits}</p>
      <p className="dialog-tip">{t("ClonePhraseTip")}</p>
      <Problem message={step.problem} />
      <div className="clone-actions">
        <RecordKey
          isRecording={step.isRecording}
          onStart={() => void commands.startClonePhrase()}
          onStop={() => void commands.finishClonePhrase()}
        />
      </div>
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
      {step.step === "terms" && <Terms clauses={step.clauses} />}
      {step.step === "sample" && <Sample step={step} />}
      {step.step === "phrase" && <Phrase step={step} />}
      {step.step === "analyzing" && <Analyzing />}
      {step.step === "name" && <NameVoice />}
    </Dialog>
  );
}
