import { createContext, useContext } from "react";
import { en } from "./en";
import { pl } from "./pl";

export type Lang = "pl" | "en";
export type TextKey = keyof typeof pl;

const DICTS: Record<Lang, Record<TextKey, string>> = { pl, en };

/** `system` → Polish when the system language is Polish, English otherwise. */
export function resolveLang(setting: string, systemLanguage: string): Lang {
  if (setting === "pl" || setting === "en") return setting;
  return systemLanguage.toLowerCase().startsWith("pl") ? "pl" : "en";
}

export function translate(lang: Lang, key: TextKey, params?: Record<string, string | number>): string {
  let text = DICTS[lang][key] ?? key;
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.split(`{${name}}`).join(String(value));
    }
  }
  return text;
}

/** Plural form: picks `<base>.one|few|many|other` by the language's rules (1 oferta, 2 oferty, 5 ofert). */
export function plural(lang: Lang, base: string, count: number): TextKey {
  const form = new Intl.PluralRules(lang === "pl" ? "pl-PL" : "en-GB").select(count);
  const key = `${base}.${form}` as TextKey;
  return key in DICTS[lang] ? key : (`${base}.other` as TextKey);
}

export type T = (key: TextKey, params?: Record<string, string | number>) => string;

export const I18nContext = createContext<{ lang: Lang; t: T }>({
  lang: "pl",
  t: (key, params) => translate("pl", key, params),
});

export const useI18n = () => useContext(I18nContext);

export { DICTS };
