/** Listening this long with nothing to show for it deserves a word about why. */
export const HINT_AFTER_MS = 6000;
/** Below this the microphone is as good as unplugged. */
export const SILENT_LEVEL = 0.01;

export type ListeningHint = "AiNoSound" | "AiNotAVoiceYet";

/**
 * What to tell a user whose talk is not being counted: nothing until a while has passed, then
 * that the microphone is silent, or that what it hears is not a voice. `peakLevel` is the
 * loudest the microphone got since the listening began, from 0 to 1.
 */
export function listeningHint(heardPercent: number, peakLevel: number, elapsedMs: number): ListeningHint | null {
  if (heardPercent > 0 || elapsedMs < HINT_AFTER_MS) return null;
  return peakLevel < SILENT_LEVEL ? "AiNoSound" : "AiNotAVoiceYet";
}
