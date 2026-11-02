import { open } from "@tauri-apps/plugin-dialog";
import type { Translate } from "./store/text";
import { whileHeld } from "./ui/hotkeyPause";

const AUDIO_EXTENSIONS = ["mp3", "wav", "m4a", "aac", "flac"];

/** Asks for audio files on the computer. The global keys rest while the picker is open. */
export async function pickAudioFiles(t: Translate, multiple: boolean): Promise<string[]> {
  const filters = [
    { name: t("AudioFileType"), extensions: AUDIO_EXTENSIONS },
    { name: t("AllFiles"), extensions: ["*"] },
  ];
  const picked = await whileHeld(() => open({ filters, multiple }));
  if (picked === null) return [];
  return Array.isArray(picked) ? picked : [picked];
}
