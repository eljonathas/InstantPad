import type { CSSProperties } from "react";
import { useText } from "../../shared/store/text";
import { EFFECTS, type Effect } from "./effects";
import "../board/Pad.css";
import "./Voice.css";

interface VoiceBoardProps {
  active: string[];
  selected: string;
  onPress: (effect: Effect) => void;
}

const EMPTY_SLOTS = 5;

/** The effects, on the same tactile pads as the sounds. A lit pad is an effect in use. */
export function VoiceBoard({ active, selected, onPress }: VoiceBoardProps) {
  const { t } = useText();

  return (
    <div className="voice-board">
      {EFFECTS.map((effect) => (
        <button
          key={effect.id}
          type="button"
          className="pad"
          style={{ "--pad-color": effect.color, "--pad-contrast": "var(--color-black)" } as CSSProperties}
          data-loaded=""
          data-target={selected === effect.id ? "" : undefined}
          data-playing={active.includes(effect.id) ? "" : undefined}
          onPointerDown={() => onPress(effect)}
        >
          <span className="pad-body">
            <i className="pad-sheen" />
            <span className="pad-name">{t(effect.name)}</span>
          </span>
        </button>
      ))}
      {Array.from({ length: EMPTY_SLOTS }, (_, slot) => (
        <div key={slot} className="pad voice-empty">
          <span className="pad-body">
            <i className="pad-sheen" />
          </span>
        </div>
      ))}
    </div>
  );
}
