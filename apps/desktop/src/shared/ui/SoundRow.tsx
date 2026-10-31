import { commands, type SoundSource } from "../ipc";
import { useBackend } from "../store/backend";
import { useText } from "../store/text";
import { beginDrag } from "./drag";
import "./SoundRow.css";

/** The star follows the favorites list, which compares paths without regard to case. */
function useIsFavorite(path: string) {
  const favorites = useBackend((state) => state.favorites);
  const wanted = path.toLowerCase();
  return favorites.some((favorite) => favorite.path.toLowerCase() === wanted);
}

/** A sound of the catalog or of the favorites. Dragging the row onto a pad puts it there. */
export function SoundRow({ source }: { source: SoundSource }) {
  const { t } = useText();
  const isFavorite = useIsFavorite(source.path);

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
      <button className="key sound-icon" title={t("PreviewOnHeadphones")} onClick={() => commands.previewSound(source)}>
        ▶
      </button>
      <button className="key sound-mic" title={t("PlayInGame")} onClick={() => commands.playInGame(source)}>
        {t("Mic")}
      </button>
      <button
        className="key key-accent sound-pad"
        title={t("SaveToPad")}
        onClick={() => commands.addToPad(source, null)}
      >
        {t("AddToPad")}
      </button>
      <button className="key key-star sound-icon" title={t("Favorite")} onClick={() => commands.toggleFavorite(source)}>
        {isFavorite ? "★" : "☆"}
      </button>
    </li>
  );
}
