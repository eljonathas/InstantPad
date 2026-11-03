import { useCallback, useEffect, useRef, useState } from "react";
import { pickAudioFiles } from "../../shared/audioFiles";
import { commands, type Failure, type SoundSource } from "../../shared/ipc";
import { regionName } from "../../shared/i18n";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { holdHotkeys } from "../../shared/ui/hotkeyPause";
import { SoundRow } from "../../shared/ui/SoundRow";
import { SoundSkeletons } from "./SoundSkeletons";
import "./Browser.css";

type Status = { kind: "loading" } | { kind: "shown" } | { kind: "failed"; reason: Failure };

interface Results {
  /** What the list shows: a search, or the trending sounds when null. */
  search: string | null;
  sounds: SoundSource[];
  page: number;
  hasMore: boolean;
  status: Status;
}

const SKELETONS_OF_A_LIST = 12;
const SKELETONS_OF_MORE = 3;
/** The next page starts loading this far before the end of the list comes into view. */
const LOAD_AHEAD = "0px 0px 240px 0px";

const NOTHING: Results = { search: null, sounds: [], page: 1, hasMore: false, status: { kind: "loading" } };

/** The MyInstants screen: search, trending sounds and paging, and sounds from the computer. */
export function Browser() {
  const { t, render, language } = useText();
  const started = useBackend((state) => state.started);
  const region = useBackend((state) => state.setup.trendingRegion);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState(NOTHING);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const end = useRef<HTMLLIElement>(null);
  const holding = useRef<(() => void) | null>(null);
  const latest = useRef(0);

  // Only the newest request may answer: a slow earlier one must not overwrite it.
  const load = useCallback(async (search: string | null, page: number) => {
    const request = ++latest.current;
    const answer = await (search === null ? commands.trending(page) : commands.search(search, page));
    if (request !== latest.current) return;

    setResults((before) =>
      answer.status === "ok"
        ? {
            search,
            page,
            sounds: page === 1 ? answer.data.sounds : [...before.sounds, ...answer.data.sounds],
            hasMore: answer.data.hasMore,
            status: { kind: "shown" },
          }
        : { ...before, status: { kind: "failed", reason: answer.error } },
    );
  }, []);

  // The first load waits for the backend to start, and a new country brings its trending sounds.
  useEffect(() => {
    if (started) void load(null, 1);
  }, [started, region, load]);

  const show = useCallback(
    (search: string | null, page: number) => {
      setResults((before) => ({ ...before, search, page, hasMore: false, status: { kind: "loading" } }));
      if (page === 1) list.current?.scrollTo({ top: 0 });
      void load(search, page);
    },
    [load],
  );

  // The next page loads when the end of the list comes near, so scrolling never runs out
  // while there is more. A list too short to scroll keeps loading until it fills the panel.
  const canLoadMore = results.hasMore && results.status.kind === "shown";
  useEffect(() => {
    const target = end.current;
    if (!canLoadMore || !target) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        show(results.search, results.page + 1);
      },
      { root: list.current, rootMargin: LOAD_AHEAD },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [canLoadMore, results.search, results.page, show]);

  useEffect(() => () => holding.current?.(), []);

  function search() {
    // Hands the numeric keys back to the pads as soon as the typing is over.
    input.current?.blur();
    if (query.trim()) show(query.trim(), 1);
  }

  async function addFromComputer() {
    const paths = await pickAudioFiles(t, true);
    if (paths.length > 0) await commands.importFiles(paths, null);
  }

  const { status, sounds, search: shown } = results;
  const isLoading = status.kind === "loading";
  const isNewList = isLoading && results.page === 1;
  // A next page that failed waits for the user, so a lost connection is not asked again and again.
  const failedMore = status.kind === "failed" && results.page > 1;
  const line =
    isLoading
      ? t("Loading")
      : status.kind === "failed"
        ? render(status.reason)
        : sounds.length === 0
          ? t("NothingFound")
          : `${shown ?? t("TrendingTitle", [regionName(language, region)])} (${sounds.length})`;

  return (
    <section className="browser" aria-label="MyInstants">
      <div className="browser-search">
        <input
          ref={input}
          className="field"
          aria-label={t("Search")}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && search()}
          onFocus={() => (holding.current ??= holdHotkeys())}
          onBlur={() => {
            holding.current?.();
            holding.current = null;
          }}
        />
        <button className="key key-bare browser-key" title={t("Search")} aria-label={t("Search")} onClick={search}>
          <i className="icon icon-search" />
        </button>
        <button
          className="key key-bare browser-key"
          title={t("FromComputerHint")}
          aria-label={t("FromComputer")}
          onClick={() => void addFromComputer()}
        >
          <i className="icon icon-folder" />
        </button>
      </div>
      <div className="browser-status">
        <span className="screen-text browser-line" role="status">
          {line}
        </span>
        <button
          className="key key-bare browser-key"
          title={t("Trending")}
          aria-label={t("Trending")}
          onClick={() => show(null, 1)}
        >
          <i className="icon icon-trending" />
        </button>
      </div>
      <ul ref={list} className="sound-list" aria-busy={isLoading}>
        {!isNewList && sounds.map((sound) => <SoundRow key={sound.path} source={sound} />)}
        {isLoading && <SoundSkeletons count={isNewList ? SKELETONS_OF_A_LIST : SKELETONS_OF_MORE} />}
        {canLoadMore && <li ref={end} className="sound-list-end" aria-hidden="true" />}
      </ul>
      {failedMore && (
        <button className="key browser-more" onClick={() => show(shown, results.page)}>
          {t("TryAgain")}
        </button>
      )}
    </section>
  );
}
