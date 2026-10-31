import { useState } from "react";
import "./RollingText.css";

/** A line of text where a new value rises from below and pushes the old one out of the top. */
export function RollingText({ text, className }: { text: string; className?: string }) {
  const [shown, setShown] = useState(text);
  const [leaving, setLeaving] = useState<string | null>(null);

  if (shown !== text) {
    setShown(text);
    setLeaving(shown);
  }

  return (
    <span className={className ? `rolling ${className}` : "rolling"}>
      <span key={shown} className="rolling-term" data-entering={leaving ? "" : undefined}>
        {shown}
      </span>
      {leaving && (
        <span
          key={leaving}
          className="rolling-term rolling-leaving"
          aria-hidden
          onAnimationEnd={() => setLeaving(null)}
        >
          {leaving}
        </span>
      )}
    </span>
  );
}
