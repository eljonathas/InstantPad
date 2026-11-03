import type { TextId } from "../../shared/i18n";

const MINUTE = 60;

/** A long stretch of time, rounded to what a person plans with: hours and minutes, or minutes. */
export function duration(seconds: number): [TextId, string[]] {
  const minutes = Math.max(1, Math.round(seconds / MINUTE));
  if (minutes < 60) return ["DurationMinutes", [String(minutes)]];
  return ["DurationHours", [String(Math.floor(minutes / 60)), String(minutes % 60)]];
}

/** Recorded speech as minutes and seconds, such as "12:05". */
export function clock(seconds: number): string {
  const whole = Math.floor(seconds);
  const rest = whole % MINUTE;
  return `${Math.floor(whole / MINUTE)}:${rest < 10 ? "0" : ""}${rest}`;
}
