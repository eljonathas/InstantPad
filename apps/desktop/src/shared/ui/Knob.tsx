import { useId, useRef, type KeyboardEvent, type PointerEvent, type WheelEvent } from "react";
import "./Knob.css";
import { clamp, degreesFor, isTickLit, pointOn, TICK_COUNT, valueAfterDrag, valueAfterWheel } from "./knobMath";

// Geometry in viewBox units. The CSS token --knob-size scales the whole drawing.
const SIZE = 92;
const CENTER = SIZE / 2;
const RADIUS = CENTER - 9;
const TICK_OUTER = RADIUS + 8;
const TICK_LENGTH = 3;
const SHADOW_OFFSET = 3;
const CAP_RATIO = 0.78;
const POINTER_FROM = 0.34;
const POINTER_TO = 0.74;
const KEY_STEP = 0.04;

interface KnobProps {
  label: string;
  value: number;
  defaultValue: number;
  onChange: (value: number) => void;
}

export function Knob({ label, value, defaultValue, onChange }: KnobProps) {
  const ids = useId();
  const drag = useRef<{ startY: number; startValue: number; scale: number } | null>(null);
  const fraction = clamp(value);

  function begin(event: PointerEvent<SVGSVGElement>) {
    if (event.button !== 0) return;
    const box = event.currentTarget.getBoundingClientRect();
    // The device is scaled to fit the window, so a screen pixel is not a knob pixel.
    drag.current = { startY: event.clientY, startValue: value, scale: box.height / SIZE };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function move(event: PointerEvent<SVGSVGElement>) {
    if (!drag.current) return;
    const { startY, startValue, scale } = drag.current;
    onChange(valueAfterDrag(startValue, (startY - event.clientY) / scale));
  }

  function key(event: KeyboardEvent<SVGSVGElement>) {
    const direction = { ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1 }[event.key];
    if (!direction) return;
    event.preventDefault();
    onChange(clamp(value + direction * KEY_STEP));
  }

  const pointerAngle = degreesFor(fraction);
  const from = pointOn(CENTER, CENTER, RADIUS * POINTER_FROM, pointerAngle);
  const to = pointOn(CENTER, CENTER, RADIUS * POINTER_TO, pointerAngle);

  return (
    <svg
      className="knob"
      viewBox={`0 0 ${SIZE} ${SIZE}`}
      role="slider"
      tabIndex={0}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(fraction * 100)}
      data-nodrag
      onPointerDown={begin}
      onPointerMove={move}
      onPointerUp={() => (drag.current = null)}
      onPointerCancel={() => (drag.current = null)}
      onDoubleClick={() => onChange(defaultValue)}
      onWheel={(event: WheelEvent) => onChange(valueAfterWheel(value, -event.deltaY))}
      onKeyDown={key}
    >
      <defs>
        <linearGradient id={`${ids}skirt`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" className="knob-skirt-top" />
          <stop offset="1" className="knob-skirt-bottom" />
        </linearGradient>
        <radialGradient id={`${ids}cap`} cx="0.5" cy="0.5" r="0.5" fx="0.35" fy="0.25">
          <stop offset="0" className="knob-cap-light" />
          <stop offset="1" className="knob-cap-dark" />
        </radialGradient>
      </defs>

      {Array.from({ length: TICK_COUNT }, (_, tick) => {
        const angle = degreesFor(tick / (TICK_COUNT - 1));
        const inner = pointOn(CENTER, CENTER, TICK_OUTER - TICK_LENGTH, angle);
        const outer = pointOn(CENTER, CENTER, TICK_OUTER, angle);
        return (
          <line
            key={tick}
            className={isTickLit(tick, fraction) ? "knob-tick knob-tick-lit" : "knob-tick"}
            x1={inner.x}
            y1={inner.y}
            x2={outer.x}
            y2={outer.y}
          />
        );
      })}

      <circle className="knob-shadow" cx={CENTER} cy={CENTER + SHADOW_OFFSET} r={RADIUS + 2} />
      <circle className="knob-skirt" cx={CENTER} cy={CENTER} r={RADIUS} fill={`url(#${ids}skirt)`} />
      <circle className="knob-cap" cx={CENTER} cy={CENTER} r={RADIUS * CAP_RATIO} fill={`url(#${ids}cap)`} />
      <line className="knob-pointer" x1={from.x} y1={from.y} x2={to.x} y2={to.y} />
    </svg>
  );
}
