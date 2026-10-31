// The math of LedMeter.cs: sixteen segments, green at the bottom, then amber, then red.
export const SEGMENTS = 16;
const RED_SEGMENTS = 2;
const AMBER_SEGMENTS = 3;

export type Tone = "green" | "amber" | "red";

/** How many segments, counted from the bottom, a level lights. Any sound lights at least one. */
export const litSegments = (level: number) => Math.ceil(Math.min(1, Math.max(0, level)) * SEGMENTS);

export function toneOf(segment: number): Tone {
  if (segment >= SEGMENTS - RED_SEGMENTS) return "red";
  return segment >= SEGMENTS - RED_SEGMENTS - AMBER_SEGMENTS ? "amber" : "green";
}
