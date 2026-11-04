import type { Translate } from "../store/text";
import type { MenuEntry } from "./ContextMenu";

export interface SoundMenuActions {
  playInGame: () => void;
  addToPad: () => void;
  cut: () => void;
}

/**
 * The options of a sound of the catalog or of the favorites, behind the button at the end of its
 * row. Listening and the star stay on the row itself.
 */
export function soundMenu(t: Translate, run: SoundMenuActions): MenuEntry[] {
  return [
    { label: t("PlayInGame"), run: run.playInGame },
    { label: t("SaveToPad"), run: run.addToPad },
    "separator",
    { label: t("MenuCutIntoPads"), run: run.cut },
  ];
}
