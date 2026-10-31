import type { CSSProperties } from "react";
import { useText } from "../../shared/store/text";
import { EffectIcon } from "./EffectIcon";
import { EFFECTS, type Effect } from "./effects";
import "./Voice.css";

interface EffectListProps {
  selected: string;
  onSelect: (effect: Effect) => void;
}

/** The voices, each a round key that stays down while its effect is on. */
export function EffectList({ selected, onSelect }: EffectListProps) {
  const { t } = useText();

  return (
    <div className="effect-screen">
      <h3 className="screen-text effect-heading">{t("VoiceEffects")}</h3>
      <ul className="effect-list">
        {EFFECTS.map((effect) => (
          <li key={effect.id}>
            <button
              type="button"
              className="effect"
              style={{ "--effect-color": effect.color } as CSSProperties}
              aria-pressed={selected === effect.id}
              onPointerDown={() => onSelect(effect)}
            >
              <span className="effect-well">
                <span className="effect-cap">
                  <EffectIcon name={effect.icon} />
                </span>
              </span>
              <span className="screen-text effect-name">{t(effect.name)}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
