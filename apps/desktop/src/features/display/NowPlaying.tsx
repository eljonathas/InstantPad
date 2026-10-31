import { useEffect, useRef } from "react";
import { meter } from "../../shared/store/meter";

const clock = (seconds: number) => {
  const whole = Math.floor(seconds);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
};

/** Repainted straight in the DOM on every frame, so the 30 Hz stream never re-renders React. */
export function NowPlaying() {
  const root = useRef<HTMLDivElement>(null);
  const bars = useRef<HTMLDivElement>(null);
  const time = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    const paint = () => {
      const playing = meter.read().nowPlaying;
      root.current!.hidden = !playing;
      if (!playing) return;

      const { position, duration, peaks } = playing;
      time.current!.textContent = `${clock(position)} / ${clock(duration)}`;

      const cells = bars.current!.children as HTMLCollectionOf<HTMLElement>;
      if (cells.length !== peaks.length) {
        bars.current!.replaceChildren(...peaks.map(() => document.createElement("i")));
      }
      const reached = (position / duration) * peaks.length;
      peaks.forEach((peak, index) => {
        cells[index].style.height = `${peak * 100}%`;
        cells[index].toggleAttribute("data-played", index < reached);
      });
    };

    paint();
    return meter.subscribe(paint);
  }, []);

  return (
    <div ref={root} className="now-playing" hidden>
      <div ref={bars} className="now-playing-bars" aria-hidden />
      <span ref={time} className="now-playing-time" />
    </div>
  );
}
