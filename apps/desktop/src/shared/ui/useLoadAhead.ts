import { useEffect, useRef, type RefObject } from "react";

/** The next page starts loading this far before the end of the list comes into view. */
const LOAD_AHEAD = "0px 0px 240px 0px";

/**
 * Calls `loadMore` when the end of a scrolling list comes near, so scrolling never runs out while
 * there is more; null when there is nothing more to load. `list` is the scrolling element, and the
 * returned ref goes on an empty item drawn after the last row while there is more. A list too short
 * to scroll keeps loading until it fills its panel, since a new `loadMore` watches again.
 */
export function useLoadAhead(list: RefObject<HTMLUListElement | null>, loadMore: (() => void) | null) {
  const end = useRef<HTMLLIElement>(null);

  useEffect(() => {
    const target = end.current;
    if (!loadMore || !target) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        loadMore();
      },
      { root: list.current, rootMargin: LOAD_AHEAD },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [list, loadMore]);

  return end;
}
