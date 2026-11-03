import { useId } from "react";

interface SwitchProps {
  label: string;
  /** A line under the label that says what the switch does. */
  hint?: string;
  on: boolean;
  onChange: (on: boolean) => void;
}

/** A setting that is on or off: its text on the left, a small switch on the right. */
export function Switch({ label, hint, on, onChange }: SwitchProps) {
  const hintId = useId();

  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={on}
      aria-label={label}
      aria-describedby={hint ? hintId : undefined}
      onClick={() => onChange(!on)}
    >
      <span className="switch-text">
        <span className="switch-label screen-text">{label}</span>
        {hint && (
          <span id={hintId} className="switch-hint screen-text">
            {hint}
          </span>
        )}
      </span>
      <span className="switch-track" aria-hidden="true">
        <span className="switch-thumb" />
      </span>
    </button>
  );
}
