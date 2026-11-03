import { useEffect, useState, useSyncExternalStore } from "react";
import { commands, type AiView } from "../../shared/ipc";
import { meter } from "../../shared/store/meter";
import { useText } from "../../shared/store/text";
import { HINT_AFTER_MS, listeningHint, type ListeningHint } from "./listeningHint";

function useMicLevel() {
  return useSyncExternalStore(meter.subscribe, () => meter.read().micLevel);
}

/**
 * The microphone while the AI listens, so the user sees it is being heard, and after a while
 * with no talk counted, why not: the microphone is silent, or what it hears is not a voice.
 */
function Listening({ heardPercent }: { heardPercent: number }) {
  const { t } = useText();
  const level = useMicLevel();
  // The hint for a count of talk, so a hint found for one count is not shown with another.
  const [found, setFound] = useState<{ forPercent: number; hint: ListeningHint | null }>({
    forPercent: 0,
    hint: null,
  });
  const hint = found.forPercent === heardPercent ? found.hint : null;

  useEffect(() => {
    // The loudest the microphone got since this count began, read when the hint is due.
    let peak = 0;
    const unsubscribe = meter.subscribe(() => {
      peak = Math.max(peak, meter.read().micLevel);
    });
    const timer = setTimeout(
      () => setFound({ forPercent: heardPercent, hint: listeningHint(heardPercent, peak, HINT_AFTER_MS) }),
      HINT_AFTER_MS,
    );
    return () => {
      clearTimeout(timer);
      unsubscribe();
    };
  }, [heardPercent]);

  return (
    <>
      <p className="screen-text ai-text">{t("AiListenPrompt")}</p>
      <progress className="ai-progress" max={100} value={heardPercent} />
      <div className="ai-hearing">
        <span className="screen-text ai-hearing-label">{t("AiHearing")}</span>
        <meter className="ai-level" min={0} max={1} value={Math.min(1, level)} aria-label={t("AiHearing")} />
      </div>
      {hint && <p className="screen-text ai-hint">{t(hint)}</p>}
    </>
  );
}

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

  return <Listening heardPercent={ai.heardPercent} />;
}
