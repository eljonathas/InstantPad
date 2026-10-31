import type { CSSProperties } from "react";
import { commands } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { EffectIcon } from "./EffectIcon";
import "./Voice.css";

/** The voices, each a round key that stays down while its effect is on. */
export function EffectList() {
  const { t } = useText();
  const { active, effects } = useBackend((state) => state.voice);

  return (
    <div className="effect-screen">
      <h3 className="screen-text effect-heading">{t("VoiceEffects")}</h3>
      <ul className="effect-list">
        {effects.map(({ effect, name, icon, color }) => (
          <li key={effect}>
            <button
              type="button"
              className="effect"
              style={{ "--effect-color": color } as CSSProperties}
              aria-pressed={active === effect}
              onPointerDown={() => void commands.selectVoiceEffect(effect)}
            >
              <span className="effect-well">
                <span className="effect-cap">
                  <EffectIcon name={icon} />
                </span>
              </span>
              <span className="screen-text effect-name">{t(name)}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
