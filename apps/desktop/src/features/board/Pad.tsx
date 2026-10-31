import { memo, useEffect, useRef, type CSSProperties, type MouseEvent } from "react";
import { commands, type PadView } from "../../shared/ipc";
import { requestSources } from "../../shared/store/backend";
import { useIsPlaying } from "../../shared/store/meter";
import { beginDrag, landOnPad } from "../../shared/ui/drag";
import "./Pad.css";

/** At or below this the speaker shows one wave instead of two. */
const LOW_VOLUME = 0.5;

interface PadProps {
  pad: PadView;
  isTarget: boolean;
  onMenu: (event: MouseEvent, pad: PadView) => void;
}

/** A backlit rubber pad. It fires on press, not on release. */
export const Pad = memo(function Pad({ pad, isTarget, onMenu }: PadProps) {
  const lit = useIsPlaying(pad.slot);
  const { sound, slot, isLoading } = pad;
  const hasSound = sound !== null;
  const wasLoading = useRef(false);

  // The thud comes when the sound arrives, which is a while after the drop if it had to download.
  useEffect(() => {
    if (wasLoading.current && !isLoading && hasSound) landOnPad(slot);
    wasLoading.current = isLoading;
  }, [isLoading, hasSound, slot]);

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
      data-loading={isLoading ? "" : undefined}
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
          const item = { label: sound.name, color: sound.color, kind: "swap", source: event.currentTarget } as const;
          beginDrag({ x: event.clientX, y: event.clientY }, item, (to) => {
            landOnPad(to);
            void commands.swapPads(pad.slot, to);
          });
          return;
        }
        if (!sound) requestSources();
        void commands.pressPad(pad.slot);
      }}
    >
      {lit && <i className="pad-glow" />}
      <span className="pad-body">
        <i className="pad-sheen" />
        <span className="pad-slot">{pad.label}</span>
        <span className="pad-shortcut">{pad.shortcut}</span>
        {pad.isFavorite && <span className="pad-star">★</span>}
        {sound && (
          <span className="pad-volume" data-level={sound.volume <= LOW_VOLUME ? "low" : undefined}>
            <span>
              {Math.round(sound.volume * 100)}
              <span className="pad-volume-unit">%</span>
            </span>
          </span>
        )}
        <span className="pad-name">{sound?.name}</span>
        <i className="pad-drop-mark" />
        {isLoading && <i className="pad-loader" />}
      </span>
    </button>
  );
});
