import { useEffect, useState } from "react";
import { useText } from "../../shared/store/text";

const ELAPSED_TICK_MS = 250;

/** Counts the seconds since it appeared, so it is shown only while a recording runs. */
function Elapsed() {
  const { t } = useText();
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    const start = Date.now();
    const timer = setInterval(() => setSeconds(Math.floor((Date.now() - start) / 1000)), ELAPSED_TICK_MS);
    return () => clearInterval(timer);
  }, []);

  return <span className="record-time">{t("ElapsedSeconds", [String(seconds)])}</span>;
}

interface RecordKeyProps {
  isRecording: boolean;
  onStart: () => void;
  onStop: () => void;
}

/** One key that starts a recording and then stops it, with the time that has gone by. */
export function RecordKey({ isRecording, onStart, onStop }: RecordKeyProps) {
  const { t } = useText();

  return (
    <div className="record">
      <button
        type="button"
        className={isRecording ? "key key-stop record-key" : "key key-danger record-key"}
        onClick={isRecording ? onStop : onStart}
      >
        {t(isRecording ? "StopRecording" : "Record")}
      </button>
      {isRecording && <Elapsed />}
    </div>
  );
}
