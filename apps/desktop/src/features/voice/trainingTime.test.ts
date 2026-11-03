import { describe, expect, it } from "vitest";
import { clock, duration } from "./trainingTime";

describe("training time", () => {
  it("rounds a long stretch to hours and minutes", () => {
    expect(duration(30 * 3600 + 20 * 60 + 10)).toEqual(["DurationHours", ["30", "20"]]);
    expect(duration(45 * 60)).toEqual(["DurationMinutes", ["45"]]);
  });

  it("never says zero minutes", () => {
    expect(duration(5)).toEqual(["DurationMinutes", ["1"]]);
  });

  it("shows recorded speech as minutes and seconds", () => {
    expect(clock(725.6)).toBe("12:05");
    expect(clock(2400)).toBe("40:00");
  });
});
