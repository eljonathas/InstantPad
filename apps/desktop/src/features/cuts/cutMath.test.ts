import { describe, expect, it } from "vitest";
import { MIN_CUT_MS, MIN_VIEW_MS, clampView, clock, msToX, pan, pieceAt, piecesOf, xToMs, zoom } from "./cutMath";

describe("pieces between cues", () => {
  it("runs from each cue to the next, and from the last to the end", () => {
    expect(piecesOf([0, 1000, 2500], 4000)).toEqual([
      { startMs: 0, endMs: 1000 },
      { startMs: 1000, endMs: 2500 },
      { startMs: 2500, endMs: 4000 },
    ]);
  });

  it("leaves out what comes before the first cue, and takes the cues in any order", () => {
    expect(piecesOf([1500, 500], 3000)).toEqual([
      { startMs: 500, endMs: 1500 },
      { startMs: 1500, endMs: 3000 },
    ]);
  });

  it("leaves out cues outside the sound, the same cue twice and cues too close", () => {
    expect(piecesOf([0, 0, 1000, 1000 + MIN_CUT_MS - 1, 5000, -5], 3000)).toEqual([
      { startMs: 0, endMs: 1000 },
      { startMs: 1000, endMs: 3000 },
    ]);
  });

  it("gives the room of a cue too close to the end to the piece before", () => {
    expect(piecesOf([0, 2990], 3000)).toEqual([{ startMs: 0, endMs: 3000 }]);
    expect(piecesOf([], 3000)).toEqual([]);
  });

  it("finds the piece a moment falls in", () => {
    const pieces = piecesOf([500, 1500], 3000);
    expect(pieceAt(pieces, 100)).toBe(-1);
    expect(pieceAt(pieces, 500)).toBe(0);
    expect(pieceAt(pieces, 1499)).toBe(0);
    expect(pieceAt(pieces, 2999)).toBe(1);
  });
});

describe("the view of the waveform", () => {
  const view = { startMs: 1000, endMs: 3000 };

  it("turns moments into pixels and back", () => {
    expect(msToX(2000, view, 400)).toBe(200);
    expect(xToMs(200, view, 400, 10_000)).toBe(2000);
    expect(xToMs(-50, { startMs: 0, endMs: 1000 }, 400, 1000)).toBe(0);
    expect(xToMs(500, { startMs: 0, endMs: 1000 }, 400, 1000)).toBe(1000);
  });

  it("zooms around a moment that stays under the same pixel", () => {
    const closer = zoom(view, 0.5, 1500, 10_000);
    expect(closer).toEqual({ startMs: 1250, endMs: 2250 });
    expect(msToX(1500, closer, 400)).toBeCloseTo(msToX(1500, view, 400));
  });

  it("never zooms closer than the minimum nor out past the sound", () => {
    const close = zoom(view, 0.0001, 2000, 10_000);
    expect(close.endMs - close.startMs).toBe(MIN_VIEW_MS);
    expect(zoom(view, 1000, 2000, 10_000)).toEqual({ startMs: 0, endMs: 10_000 });
  });

  it("pans as far as the sound goes and keeps its length", () => {
    expect(pan(view, 500, 10_000)).toEqual({ startMs: 1500, endMs: 3500 });
    expect(pan(view, -5000, 10_000)).toEqual({ startMs: 0, endMs: 2000 });
    expect(pan(view, 50_000, 10_000)).toEqual({ startMs: 8000, endMs: 10_000 });
  });

  it("fits a sound shorter than the minimum view whole", () => {
    expect(clampView({ startMs: 0, endMs: 50 }, 120)).toEqual({ startMs: 0, endMs: 120 });
  });
});

describe("the clock", () => {
  it("shows minutes, seconds and hundredths", () => {
    expect(clock(0)).toBe("0:00.00");
    expect(clock(1234)).toBe("0:01.23");
    expect(clock(62_505)).toBe("1:02.50");
  });
});
