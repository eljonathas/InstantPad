import { useEffect, useRef } from "react";
import "./LedMeter.css";
import { litSegments, SEGMENTS, toneOf } from "./ledMath";
import { meter } from "../store/meter";

const segments = Array.from({ length: SEGMENTS }, (_, index) => index);

/** Repainted straight in the DOM on every frame, so the 30 Hz stream never re-renders React. */
export function LedMeter({ channel, label }: { channel: "micLevel" | "outputLevel"; label: string }) {
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const cells = Array.from(root.current!.children) as HTMLElement[];
    let lit = 0;

    const paint = () => {
      const next = litSegments(meter.read()[channel]);
      if (next === lit) return;
      cells.forEach((cell, index) => cell.toggleAttribute("data-lit", index < next));
      lit = next;
    };

    paint();
    return meter.subscribe(paint);
  }, [channel]);

  return (
    <div ref={root} className="led-meter" role="meter" aria-label={label} data-nodrag>
      {segments.map((segment) => (
        <i key={segment} className={`led led-${toneOf(segment)}`} />
      ))}
    </div>
  );
}
