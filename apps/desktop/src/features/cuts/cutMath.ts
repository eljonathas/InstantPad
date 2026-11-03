/** Shorter than this is a click, not a sound worth a pad. The backend has the same rule. */
export const MIN_CUT_MS = 50;
/** The closest the editor zooms in: this much of the sound across the whole width. */
export const MIN_VIEW_MS = 200;

export interface Piece {
  startMs: number;
  endMs: number;
}

/** The part of the sound on screen. */
export interface View {
  startMs: number;
  endMs: number;
}

/**
 * The pieces between cue points, as the backend cuts them: from each cue to the next, the last
 * to the end of the sound. Cues outside the sound, the same cue twice and a cue too close to the
 * one before are left out, and so is a last cue too close to the end.
 */
export function piecesOf(cues: number[], durationMs: number): Piece[] {
  const starts: number[] = [];
  for (const cue of cues.filter((cue) => cue >= 0 && cue < durationMs).sort((a, b) => a - b)) {
    if (starts.length === 0 || cue - starts[starts.length - 1] >= MIN_CUT_MS) starts.push(cue);
  }
  while (starts.length > 0 && durationMs - starts[starts.length - 1] < MIN_CUT_MS) starts.pop();
  return starts.map((startMs, index) => ({ startMs, endMs: starts[index + 1] ?? durationMs }));
}

/** The piece that `ms` falls in, or -1 before the first cue. */
export function pieceAt(pieces: Piece[], ms: number): number {
  return pieces.findIndex((piece) => ms >= piece.startMs && ms < piece.endMs);
}

/** Where `ms` is drawn, in pixels from the left of a waveform `width` wide. */
export function msToX(ms: number, view: View, width: number): number {
  return ((ms - view.startMs) / (view.endMs - view.startMs)) * width;
}

/** The moment under the pixel `x`, kept inside the sound. */
export function xToMs(x: number, view: View, width: number, durationMs: number): number {
  const ms = view.startMs + (x / width) * (view.endMs - view.startMs);
  return Math.round(Math.min(durationMs, Math.max(0, ms)));
}

/** Keeps a view inside the sound and no closer than [MIN_VIEW_MS], keeping its length when it can. */
export function clampView(view: View, durationMs: number): View {
  const length = Math.min(durationMs, Math.max(MIN_VIEW_MS, view.endMs - view.startMs));
  const startMs = Math.min(Math.max(0, view.startMs), durationMs - length);
  return { startMs, endMs: startMs + length };
}

/** Zooms by `factor` (below 1 is closer) around `anchorMs`, which stays under the same pixel. */
export function zoom(view: View, factor: number, anchorMs: number, durationMs: number): View {
  const share = (anchorMs - view.startMs) / (view.endMs - view.startMs);
  const length = (view.endMs - view.startMs) * factor;
  const startMs = anchorMs - share * length;
  return clampView({ startMs, endMs: startMs + length }, durationMs);
}

/** Moves the view by `deltaMs`, as far as the sound goes. */
export function pan(view: View, deltaMs: number, durationMs: number): View {
  return clampView({ startMs: view.startMs + deltaMs, endMs: view.endMs + deltaMs }, durationMs);
}

/** Minutes, seconds and hundredths, such as "1:02.50". */
export function clock(ms: number): string {
  const hundredths = Math.floor(ms / 10);
  const seconds = Math.floor(hundredths / 100);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}.${String(hundredths % 100).padStart(2, "0")}`;
}
