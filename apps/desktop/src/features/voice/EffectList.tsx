import type { CSSProperties } from "react";
import { commands, type EffectView } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { isAiReady, selectEffect, type PendingAi } from "./ai";
import { EffectIcon } from "./EffectIcon";
import "./Voice.css";

/** The voices, each a round key that stays down while its effect is on. */
export function EffectList({ onPending }: { onPending: (pending: PendingAi | null) => void }) {
  const { t, render } = useText();
  const { active, activeCloneId, effects, ai } = useBackend((state) => state.voice);
  const isReady = isAiReady(ai);

  function pick(item: EffectView) {
    void selectEffect(item);
    onPending(item.needsAi && !isReady ? { name: item.name, apply: () => void selectEffect(item) } : null);
  }

  function cloneVoice() {
    void commands.openCloning();
    // Cloning needs the models, not the user's own voice, so only a missing download waits.
    onPending(ai.status.kind === "ready" ? null : { name: "CloneVoice", apply: () => void commands.openCloning() });
  }

  return (
    <div className="effect-screen">
      <h3 className="screen-text effect-heading">{t("VoiceEffects")}</h3>
      <ul className="effect-list">
        {effects.map((item) => (
          <li key={item.cloneId ?? item.effect}>
            <button
              type="button"
              className="effect"
              style={{ "--effect-color": item.color } as CSSProperties}
              aria-pressed={item.effect === active && item.cloneId === activeCloneId}
              onPointerDown={() => pick(item)}
            >
              <span className="effect-well">
                <span className="effect-cap">
                  <EffectIcon name={item.icon} />
                </span>
                {item.needsAi && (
                  <span className="effect-ai" aria-hidden>
                    {t("AiMark")}
                  </span>
                )}
              </span>
              <span className="screen-text effect-name">{render(item.name)}</span>
            </button>
          </li>
        ))}
        <li>
          <button type="button" className="effect" onClick={cloneVoice}>
            <span className="effect-well">
              <span className="effect-cap">
                <i className="icon icon-plus effect-plus" />
              </span>
            </span>
            <span className="screen-text effect-name">{t("CloneVoice")}</span>
          </button>
        </li>
      </ul>
    </div>
  );
}
