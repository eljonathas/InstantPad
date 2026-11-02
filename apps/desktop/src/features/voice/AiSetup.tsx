import { commands, type AiView } from "../../shared/ipc";
import { useText } from "../../shared/store/text";
import { RecordKey } from "./RecordKey";

/** What an AI voice still needs: the models, then the user's own voice. */
export function AiSetup({ ai }: { ai: AiView }) {
  const { t } = useText();
  const { status } = ai;

  if (status.kind === "missing") {
    return (
      <>
        <p className="screen-text ai-text">{t("AiDownloadPrompt", [String(ai.downloadMegabytes)])}</p>
        <button type="button" className="key key-accent ai-key" onClick={() => void commands.installVoiceAi()}>
          {t("AiDownload")}
        </button>
      </>
    );
  }

  if (status.kind === "downloading") {
    return (
      <>
        <p className="screen-text ai-text">{t("AiDownloading", [String(status.percent)])}</p>
        <progress className="ai-progress" max={100} value={status.percent} />
      </>
    );
  }

  return (
    <>
      <p className="screen-text ai-text">{t("AiCalibratePrompt", [String(ai.minCalibrationSeconds)])}</p>
      <RecordKey
        isRecording={ai.isCalibrating}
        onStart={() => void commands.startCalibration()}
        onStop={() => void commands.finishCalibration()}
      />
    </>
  );
}
