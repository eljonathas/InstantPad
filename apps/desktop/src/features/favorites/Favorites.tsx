import { useCallback, useRef, useState } from "react";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { SoundRow } from "../../shared/ui/SoundRow";
import { useLoadAhead } from "../../shared/ui/useLoadAhead";
import "./Favorites.css";

/** Rows drawn per page: the backend sends every favorite, and a long list is drawn as it scrolls. */
const ROWS_OF_A_PAGE = 30;

export function Favorites() {
  const { t } = useText();
  const favorites = useBackend((state) => state.favorites);
  const [pages, setPages] = useState(1);
  const list = useRef<HTMLUListElement>(null);

  const shown = pages * ROWS_OF_A_PAGE;
  const nextPage = useCallback(() => setPages(pages + 1), [pages]);
  const end = useLoadAhead(list, shown < favorites.length ? nextPage : null);

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
      <ul ref={list} className="sound-list">
        {favorites.slice(0, shown).map((favorite) => (
          <SoundRow key={favorite.path} source={favorite} />
        ))}
        {shown < favorites.length && <li ref={end} className="sound-list-end" aria-hidden="true" />}
      </ul>
    </section>
  );
}
