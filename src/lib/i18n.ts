import { derived, writable } from "svelte/store";
import { createZhTWDictionary } from "./locales/zh-TW";
import { enUS } from "./locales/en-US";
import { zhCN, type TranslationKey } from "./locales/zh-CN";
import type { Locale } from "../types";

const fallbackLocale: Locale = "zh-CN";

export type { TranslationKey };

const dictionaries: Record<Locale, Record<TranslationKey, string>> = {
  "zh-CN": zhCN,
  "zh-TW": createZhTWDictionary(zhCN),
  "en-US": enUS
};

export const supportedLocales: Array<{ code: Locale; labelKey: TranslationKey }> = [
  { code: "zh-CN", labelKey: "settings.language.zhCN" },
  { code: "zh-TW", labelKey: "settings.language.zhTW" },
  { code: "en-US", labelKey: "settings.language.enUS" }
];

function normalizeLocale(value: string | null | undefined): Locale {
  return value === "en-US" || value === "zh-TW" || value === "zh-CN" ? value : fallbackLocale;
}

function initialLocale(): Locale {
  if (typeof localStorage === "undefined") {
    return fallbackLocale;
  }

  return normalizeLocale(localStorage.getItem("codestudio-lite-language"));
}

function interpolate(template: string, values?: Record<string, string | number>): string {
  if (!values) {
    return template;
  }

  return Object.entries(values).reduce(
    (text, [key, value]) => text.replaceAll(`{${key}}`, String(value)),
    template
  );
}

export const locale = writable<Locale>(initialLocale());

export const t = derived(locale, ($locale) => {
  const dictionary = dictionaries[$locale] ?? dictionaries[fallbackLocale];
  return (key: TranslationKey, values?: Record<string, string | number>) =>
    interpolate(dictionary[key] ?? dictionaries[fallbackLocale][key] ?? key, values);
});

export function setLocale(nextLocale: Locale) {
  locale.set(nextLocale);
  if (typeof localStorage !== "undefined") {
    localStorage.setItem("codestudio-lite-language", nextLocale);
  }
}
