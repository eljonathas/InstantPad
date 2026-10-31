/** Rows shaped like the sounds on their way, so the list does not jump when they arrive. */
export function SoundSkeletons({ count }: { count: number }) {
  return Array.from({ length: count }, (_, index) => (
    <li key={index} className="sound-skeleton" aria-hidden>
      <i className="skeleton sound-dot" />
      <i className="skeleton sound-skeleton-name" />
      <i className="skeleton sound-skeleton-key sound-icon" />
      <i className="skeleton sound-skeleton-key sound-mic" />
      <i className="skeleton sound-skeleton-key sound-pad" />
      <i className="skeleton sound-skeleton-key sound-icon" />
    </li>
  ));
}
