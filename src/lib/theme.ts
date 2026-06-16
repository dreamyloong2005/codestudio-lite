import type { AppSettings } from "../types";

export type ThemePreference = AppSettings["theme"];
type ResolvedTheme = Exclude<ThemePreference, "system">;

let currentPreference: ThemePreference = "system";
let mediaQuery: MediaQueryList | null = null;
let listeningForSystemTheme = false;

function resolveTheme(theme: ThemePreference): ResolvedTheme {
  if (theme === "light" || theme === "dark") {
    return theme;
  }

  if (typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: light)").matches) {
    return "light";
  }

  return "dark";
}

function ensureSystemThemeListener() {
  if (listeningForSystemTheme || typeof window === "undefined" || !window.matchMedia) {
    return;
  }

  mediaQuery = window.matchMedia("(prefers-color-scheme: light)");
  const handleChange = () => {
    if (currentPreference === "system") {
      applyTheme("system");
    }
  };

  if (mediaQuery.addEventListener) {
    mediaQuery.addEventListener("change", handleChange);
  } else {
    mediaQuery.addListener?.(handleChange);
  }

  listeningForSystemTheme = true;
}

export function applyTheme(theme: ThemePreference = "system") {
  currentPreference = theme;

  if (typeof document === "undefined") {
    return;
  }

  const resolvedTheme = resolveTheme(theme);
  document.documentElement.dataset.themePreference = theme;
  document.documentElement.dataset.theme = resolvedTheme;
  document.documentElement.style.colorScheme = resolvedTheme;
  ensureStaticFavicon();
  ensureSystemThemeListener();
}

function ensureStaticFavicon() {
  const existingLink = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
  const link = existingLink ?? document.createElement("link");
  link.rel = "icon";
  link.type = "image/svg+xml";
  link.href = "/icon.svg";
  if (!existingLink) {
    document.head.appendChild(link);
  }
}
