import type { Message } from "../ipc";
import en from "./en.json";
import pt from "./pt.json";

export type TextId = keyof typeof en;
export type Language = "en" | "pt";

const texts: Record<Language, Record<TextId, string>> = { en, pt };

export function resolveLanguage(saved: string | null, system: string): Language {
  const tag = (saved ?? system).toLowerCase();
  return tag.startsWith("pt") ? "pt" : "en";
}

export function translate(language: Language, id: TextId, args: string[] = []): string {
  return texts[language][id].replace(/\{(\d+)\}/g, (_, index) => args[Number(index)] ?? "");
}

/** The name of a country, such as "Brazil" for "br", in the language of the interface. */
export function regionName(language: Language, region: string): string {
  return new Intl.DisplayNames([language], { type: "region" }).of(region.toUpperCase()) ?? region;
}

export function renderMessage(language: Language, message: Message): string {
  if (message.kind === "plain") return message.text;
  const args = message.args.map((arg) => renderMessage(language, arg));
  return translate(language, message.key, args);
}
