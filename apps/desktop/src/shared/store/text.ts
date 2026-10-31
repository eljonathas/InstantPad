import { renderMessage, resolveLanguage, translate, type Language, type TextId } from "../i18n";
import type { Message } from "../ipc";
import { useBackend } from "./backend";

export type Translate = (id: TextId, args?: string[]) => string;

/** Every word on screen goes through here, so a language change redraws all of it. */
export function useText() {
  const saved = useBackend((state) => state.setup.language);
  const language: Language = resolveLanguage(saved, navigator.language);

  const t: Translate = (id, args) => translate(language, id, args);
  const render = (message: Message | TextId) =>
    typeof message === "string" ? translate(language, message) : renderMessage(language, message);

  return { language, t, render };
}
