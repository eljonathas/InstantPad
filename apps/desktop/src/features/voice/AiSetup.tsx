import { commands, type AiView } from "../../shared/ipc";
import { useText } from "../../shared/store/text";

/** What an AI voice still needs: the models, then to hear the user talk, which needs nothing from them. */
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

  if (ai.calibration === "analyzing") return <p className="screen-text ai-text">{t("AiLearning")}</p>;

  return (
    <>
      <p className="screen-text ai-text">{t("AiListenPrompt")}</p>
      <progress className="ai-progress" max={100} value={ai.heardPercent} />
    </>
  );
}
