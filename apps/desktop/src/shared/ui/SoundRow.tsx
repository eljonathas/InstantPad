import { useState } from "react";
import { commands, type SoundSource } from "../ipc";
import { useBackend } from "../store/backend";
import { useText } from "../store/text";
import { cutEditor } from "../../features/cuts/cutEditorStore";
import { beginDrag } from "./drag";
import { useOverlay } from "./overlay";
import { soundMenu } from "./soundMenu";
import "./SoundRow.css";

/** The star follows the favorites list, which compares paths without regard to case. */
function useIsFavorite(path: string) {
  const favorites = useBackend((state) => state.favorites);
  const wanted = path.toLowerCase();
  return favorites.some((favorite) => favorite.path.toLowerCase() === wanted);
}

/**
 * A sound of the catalog or of the favorites. Dragging the row onto a pad puts it there. Listening
 * and the star stay on the row; the other actions wait behind one button, which leaves room to the name.
 */
export function SoundRow({ source }: { source: SoundSource }) {
  const { t } = useText();
  const overlay = useOverlay();
  const isFavorite = useIsFavorite(source.path);
  const [isMenuOpen, setMenuOpen] = useState(false);
  const options = t("SoundOptions", [source.name]);

  function openMenu(button: HTMLElement) {
    const entries = soundMenu(t, {
      playInGame: () => void commands.playInGame(source),
      addToPad: () => void commands.addToPad(source, null),
      cut: () => void cutEditor.open({ kind: "catalog", source }),
    });
    const box = button.getBoundingClientRect();
    setMenuOpen(true);
    overlay.menu(box.left, box.bottom, entries, { anchor: button, onClose: () => setMenuOpen(false) });
  }

  return (
    <li
      className="sound-row"
      onPointerDown={(event) => {
        if (event.button !== 0 || (event.target as Element).closest("button")) return;
        const item = { label: source.name, color: source.color, kind: "place", source: event.currentTarget } as const;
        beginDrag({ x: event.clientX, y: event.clientY }, item, (slot) => void commands.addToPad(source, slot));
      }}
    >
      <i className="sound-dot" style={{ background: source.color }} />
      <span className="sound-name screen-text" title={source.name}>
        {source.name}
      </span>
      <button
        className="key sound-icon"
        title={t("PreviewOnHeadphones")}
        aria-label={t("PreviewOnHeadphones")}
        onClick={() => commands.previewSound(source)}
      >
        ▶
      </button>
      <button className="key key-star sound-icon" title={t("Favorite")} onClick={() => commands.toggleFavorite(source)}>
        {isFavorite ? "★" : "☆"}
      </button>
      <button
        className="key sound-icon"
        title={options}
        aria-label={options}
        aria-haspopup="menu"
        aria-expanded={isMenuOpen}
        onClick={(event) => openMenu(event.currentTarget)}
      >
        <i className="icon icon-more" />
      </button>
    </li>
  );
}
