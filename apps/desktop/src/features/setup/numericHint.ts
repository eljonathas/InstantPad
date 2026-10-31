import type { NumericScheme } from "../../shared/ipc";
import type { Translate } from "../../shared/store/text";

/** The sentence under the numeric shortcuts menu. `keys` is empty when the scheme is off. */
export function describeNumericKeys(scheme: NumericScheme, keys: string[], t: Translate) {
  if (scheme === "off") return t("NumericHintOff");

  const hint = t("NumericHint", keys);
  return scheme === "numpad" ? hint + t("NumericHintNumpad") : hint;
}
