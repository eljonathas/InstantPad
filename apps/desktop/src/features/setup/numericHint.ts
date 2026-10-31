import type { NumericScheme } from "../../shared/ipc";
import type { Translate } from "../../shared/store/text";

// What the keys of each scheme are called: first pad, last pad, next and previous interface.
// The backend only sends the stop key and the pad labels, which a custom shortcut can change, so
// the other keys come from here (they mirror NumericLayout::for_scheme).
const KEYS: Record<Exclude<NumericScheme, "off">, [string, string, string, string]> = {
  numpad: ["Num1", "Num0", "Num+", "Num-"],
  ctrlNumpad: ["Ctrl+Num1", "Ctrl+Num0", "Ctrl+Num+", "Ctrl+Num-"],
  altDigits: ["Alt+1", "Alt+0", "Alt+=", "Alt+-"],
};

/** The sentence under the numeric shortcuts menu (SetupViewModel.DescribeNumericKeys). */
export function describeNumericKeys(scheme: NumericScheme, stopShortcut: string | null, t: Translate) {
  if (scheme === "off") return t("NumericHintOff");

  const hint = t("NumericHint", [...KEYS[scheme], stopShortcut ?? ""]);
  return scheme === "numpad" ? hint + t("NumericHintNumpad") : hint;
}
