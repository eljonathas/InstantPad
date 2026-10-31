import { describe, expect, it } from "vitest";
import { clamp, degreesFor, isTickLit, pointOn, TICK_COUNT, valueAfterDrag, valueAfterWheel } from "./knobMath";

describe("knob math", () => {
  it("sweeps 270 degrees from seven o'clock to five o'clock", () => {
    expect(degreesFor(0)).toBe(135);
    expect(degreesFor(0.5)).toBe(270);
    expect(degreesFor(1)).toBe(405);
  });

  it("places points on the circle, with y growing downward", () => {
    const bottomLeft = pointOn(50, 50, 10, 135);
    expect(bottomLeft.x).toBeCloseTo(50 - Math.SQRT1_2 * 10);
    expect(bottomLeft.y).toBeCloseTo(50 + Math.SQRT1_2 * 10);
    expect(pointOn(50, 50, 10, 270).y).toBeCloseTo(40);
  });

  it("takes 180 pixels of drag for the full range", () => {
    expect(valueAfterDrag(0, 180)).toBe(1);
    expect(valueAfterDrag(0.5, 90)).toBe(1);
    expect(valueAfterDrag(0.5, -45)).toBeCloseTo(0.25);
  });

  it("never leaves the range", () => {
    expect(valueAfterDrag(0.9, 500)).toBe(1);
    expect(valueAfterDrag(0.1, -500)).toBe(0);
    expect(clamp(-3)).toBe(0);
  });

  it("moves 1/25 of the range per wheel notch, up or down", () => {
    expect(valueAfterWheel(0.5, 120)).toBeCloseTo(0.54);
    expect(valueAfterWheel(0.5, -1)).toBeCloseTo(0.46);
    expect(valueAfterWheel(1, 120)).toBe(1);
  });

  it("lights the ticks up to the value, out of 21", () => {
    const lit = (fraction: number) =>
      Array.from({ length: TICK_COUNT }, (_, tick) => isTickLit(tick, fraction)).filter(Boolean).length;

    expect(TICK_COUNT).toBe(21);
    expect(lit(0)).toBe(1);
    expect(lit(0.5)).toBe(11);
    expect(lit(1)).toBe(21);
  });
});
