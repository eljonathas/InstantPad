import type { VoiceIcon } from "../../shared/ipc";

const PATHS: Record<VoiceIcon, string[]> = {
  mic: ["M12 19v3", "M19 10v2a7 7 0 0 1-14 0v-2", "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z"],
  feather: [
    "M12.67 19a2 2 0 0 0 1.416-.588l6.154-6.172a6 6 0 0 0-8.49-8.49L5.586 9.914A2 2 0 0 0 5 11.328V18a1 1 0 0 0 1 1z",
    "M16 8 2 22",
    "M17.5 15H9",
  ],
  mountain: ["m8 3 4 8 5-5 5 15H2L8 3z"],
  wave: ["M2 13a2 2 0 0 0 2-2V7a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0V4a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0v-4a2 2 0 0 1 2-2"],
};

export function EffectIcon({ name }: { name: VoiceIcon }) {
  return (
    <svg className="effect-icon" viewBox="0 0 24 24" aria-hidden>
      {PATHS[name].map((path) => (
        <path key={path} d={path} />
      ))}
    </svg>
  );
}
