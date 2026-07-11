import { derived, writable } from "svelte/store";
import { enUS } from "./locales/en-US";
import { zhCN, type TranslationKey } from "./locales/zh-CN";
import { zhTW } from "./locales/zh-TW";
import { brandChatGPTDesktopText, chatgptDesktopGeneration } from "./chatgptDesktopBranding";
import type { Locale } from "../types";

const fallbackLocale: Locale = "en-US";

export type { TranslationKey };

const dictionaries: Record<Locale, Record<TranslationKey, string>> = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  "en-US": enUS
};

export const supportedLocales: Array<{ code: Locale; label: string }> = [
  { code: "zh-CN", label: "简体中文" },
  { code: "zh-TW", label: "繁體中文" },
  { code: "en-US", label: "English" }
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

export const t = derived([locale, chatgptDesktopGeneration], ([$locale, $generation]) => {
  const dictionary = dictionaries[$locale] ?? dictionaries[fallbackLocale];
  return (key: TranslationKey, values?: Record<string, string | number>) =>
    brandChatGPTDesktopText(
      interpolate(dictionary[key] ?? dictionaries[fallbackLocale][key] ?? key, values),
      $generation
    );
});

export function setLocale(nextLocale: Locale) {
  locale.set(nextLocale);
  if (typeof localStorage !== "undefined") {
    localStorage.setItem("codestudio-lite-language", nextLocale);
  }
}
