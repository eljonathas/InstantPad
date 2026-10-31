import { memo, type CSSProperties, type MouseEvent } from "react";
import { commands, type PadView } from "../../shared/ipc";
import { requestSources } from "../../shared/store/backend";
import { useIsPlaying } from "../../shared/store/meter";
import { beginDrag } from "../../shared/ui/drag";
import "./Pad.css";

interface PadProps {
  pad: PadView;
  isTarget: boolean;
  onMenu: (event: MouseEvent, pad: PadView) => void;
}

/** A backlit rubber pad. It fires on press, not on release. */
export const Pad = memo(function Pad({ pad, isTarget, onMenu }: PadProps) {
  const lit = useIsPlaying(pad.slot);
  const { sound } = pad;
  const style = sound
    ? ({
        "--pad-color": sound.color,
        "--pad-contrast": sound.isLight ? "var(--color-black)" : "var(--color-white)",
      } as CSSProperties)
    : undefined;

  return (
    <button
      type="button"
      className="pad"
      style={style}
      data-slot={pad.slot}
      data-loaded={sound ? "" : undefined}
      data-playing={lit ? "" : undefined}
      data-target={isTarget ? "" : undefined}
      aria-label={sound ? `${pad.label} ${sound.name}` : pad.label}
      onContextMenu={(event) => {
        // macOS turns Ctrl+click into a context menu, but Ctrl+press is the drag.
        if (event.button === 0 && event.ctrlKey) return event.preventDefault();
        onMenu(event, pad);
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        // Ctrl+drag rearranges pads; a plain press always fires the sound at once.
        if (event.ctrlKey && sound) {
          beginDrag({ x: event.clientX, y: event.clientY }, sound.name, (to) => {
            if (to !== pad.slot) void commands.swapPads(pad.slot, to);
          });
          return;
        }
        if (!sound) requestSources();
        void commands.pressPad(pad.slot);
      }}
    >
      {lit && <i className="pad-glow" />}
      <span className="pad-body">
        <i className="pad-light" />
        <i className="pad-sheen" />
        <span className="pad-slot">{pad.label}</span>
        <span className="pad-shortcut">{pad.shortcut}</span>
        {pad.isFavorite && <span className="pad-star">★</span>}
        <span className="pad-name">{sound?.name}</span>
      </span>
    </button>
  );
});
