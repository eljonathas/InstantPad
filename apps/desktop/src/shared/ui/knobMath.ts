// The math of Knob.cs. The value always runs from 0 to 1.
export const TICK_COUNT = 21;
const SWEEP_DEGREES = 270;
const START_DEGREES = 135;
const DRAG_PIXELS_FOR_FULL_RANGE = 180;
const WHEEL_STEPS = 25;

export const clamp = (value: number) => Math.min(1, Math.max(0, value));

// 0 sits at seven o'clock, the end of the sweep at five o'clock.
export const degreesFor = (fraction: number) => START_DEGREES + SWEEP_DEGREES * fraction;

export function pointOn(centerX: number, centerY: number, radius: number, degrees: number) {
  const radians = (degrees * Math.PI) / 180;
  return { x: centerX + Math.cos(radians) * radius, y: centerY + Math.sin(radians) * radius };
}

/** `pixelsUp` is how far the pointer rose since the drag began, in the knob's own pixels. */
export const valueAfterDrag = (startValue: number, pixelsUp: number) =>
  clamp(startValue + pixelsUp / DRAG_PIXELS_FOR_FULL_RANGE);

/** `direction` is positive when the wheel turns up. */
export const valueAfterWheel = (value: number, direction: number) => clamp(value + Math.sign(direction) / WHEEL_STEPS);

export const isTickLit = (tick: number, fraction: number) => tick / (TICK_COUNT - 1) <= fraction;
