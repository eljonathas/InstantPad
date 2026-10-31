import { useEffect, useRef, useState, type CSSProperties } from "react";
import "./ScrollingText.css";

const PIXELS_PER_SECOND = 45;

/** Text that fits stays still. Text that does not rolls past in a loop, like on a CDJ or an MPC. */
export function ScrollingText({ text, className }: { text: string; className: string }) {
  const box = useRef<HTMLDivElement>(null);
  const line = useRef<HTMLSpanElement>(null);
  const [size, setSize] = useState({ text: 0, room: 0 });

  useEffect(() => {
    const measure = () => setSize({ text: line.current!.offsetWidth, room: box.current!.clientWidth });
    const observer = new ResizeObserver(measure);
    observer.observe(box.current!);
    observer.observe(line.current!);
    return () => observer.disconnect();
  }, []);

  const rolls = size.text > size.room;
  const style = {
    "--scrolling-seconds": `${size.text / PIXELS_PER_SECOND}s`,
    "--scrolling-width": `${size.text}px`,
  } as CSSProperties;

  return (
    <div ref={box} className={`scrolling ${className}`}>
      <div key={text} className="scrolling-track" data-rolls={rolls ? "" : undefined} style={rolls ? style : undefined}>
        <span ref={line}>{text}</span>
        {rolls && (
          <span className="scrolling-copy" aria-hidden>
            {text}
          </span>
        )}
      </div>
    </div>
  );
}
