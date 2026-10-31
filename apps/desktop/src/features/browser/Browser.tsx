import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type Failure, type SoundSource } from "../../shared/ipc";
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

const NOTHING: Results = { search: null, sounds: [], page: 1, hasMore: false, status: { kind: "loading" } };

/** The MyInstants screen: search, trending sounds and paging. */
export function Browser() {
  const { t, render } = useText();
  const started = useBackend((state) => state.started);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState(NOTHING);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
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

  // The first load waits for the backend to start.
  useEffect(() => {
    if (started) void load(null, 1);
  }, [started, load]);

  function show(search: string | null, page: number) {
    setResults((before) => ({ ...before, search, page, hasMore: false, status: { kind: "loading" } }));
    if (page === 1) list.current?.scrollTo({ top: 0 });
    void load(search, page);
  }

  useEffect(() => () => holding.current?.(), []);

  function search() {
    // Hands the numeric keys back to the pads as soon as the typing is over.
    input.current?.blur();
    if (query.trim()) show(query.trim(), 1);
  }

  const { status, sounds, search: shown } = results;
  const isLoading = status.kind === "loading";
  const isNewList = isLoading && results.page === 1;
  const line =
    isLoading
      ? t("Loading")
      : status.kind === "failed"
        ? render(status.reason)
        : sounds.length === 0
          ? t("NothingFound")
          : `${shown ?? t("TrendingTitle")} (${sounds.length})`;

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
        <button className="key" onClick={search}>
          {t("Search")}
        </button>
      </div>
      <div className="browser-status">
        <span className="screen-text browser-line" role="status">
          {line}
        </span>
        <button className="key" onClick={() => show(null, 1)}>
          {t("Trending")}
        </button>
      </div>
      <ul ref={list} className="sound-list" aria-busy={isLoading}>
        {!isNewList && sounds.map((sound) => <SoundRow key={sound.path} source={sound} />)}
        {isLoading && <SoundSkeletons count={isNewList ? SKELETONS_OF_A_LIST : SKELETONS_OF_MORE} />}
      </ul>
      {results.hasMore && (
        <button className="key browser-more" onClick={() => show(shown, results.page + 1)}>
          {t("LoadMore")}
        </button>
      )}
    </section>
  );
}
