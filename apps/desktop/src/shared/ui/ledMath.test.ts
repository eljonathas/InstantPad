import { describe, expect, it } from "vitest";
import { litSegments, SEGMENTS, toneOf } from "./ledMath";

describe("LED meter math", () => {
  it("has sixteen segments: the top 2 red, the next 3 amber, the rest green", () => {
    const tones = Array.from({ length: SEGMENTS }, (_, segment) => toneOf(segment));

    expect(SEGMENTS).toBe(16);
    expect(tones.filter((tone) => tone === "red")).toHaveLength(2);
    expect(tones.filter((tone) => tone === "amber")).toHaveLength(3);
    expect(tones.filter((tone) => tone === "green")).toHaveLength(11);
    expect(tones.slice(-2)).toEqual(["red", "red"]);
    expect(tones.slice(11, 14)).toEqual(["amber", "amber", "amber"]);
  });

  it("rounds up, so any signal lights at least one segment", () => {
    expect(litSegments(0)).toBe(0);
    expect(litSegments(0.001)).toBe(1);
    expect(litSegments(0.5)).toBe(8);
    expect(litSegments(0.51)).toBe(9);
    expect(litSegments(1)).toBe(16);
  });

  it("ignores levels outside 0 to 1", () => {
    expect(litSegments(-1)).toBe(0);
    expect(litSegments(7)).toBe(16);
  });
});
