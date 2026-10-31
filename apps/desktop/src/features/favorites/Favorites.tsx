import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { SoundRow } from "../../shared/ui/SoundRow";
import "./Favorites.css";

export function Favorites() {
  const { t } = useText();
  const favorites = useBackend((state) => state.favorites);

  const status =
    favorites.length === 0
      ? t("FavoritesEmpty")
      : favorites.length === 1
        ? t("FavoritesOne")
        : t("FavoritesCount", [String(favorites.length)]);

  return (
    <section className="favorites" aria-label={t("FavoritesTab")}>
      <p className="screen-text favorites-status" role="status">
        {status}
      </p>
      <ul className="sound-list">
        {favorites.map((favorite) => (
          <SoundRow key={favorite.path} source={favorite} />
        ))}
      </ul>
    </section>
  );
}
