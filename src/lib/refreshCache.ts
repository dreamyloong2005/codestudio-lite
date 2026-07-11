export const REFRESH_CACHE_TTL_MS = 2 * 60 * 60_000;

const STORAGE_PREFIX = "codestudio-lite:last-refresh:";

export type RefreshCacheKey = "detection" | "chatgptDesktop" | "claudeDesktop";

function storageKey(key: RefreshCacheKey) {
  return `${STORAGE_PREFIX}${key}`;
}

export function readRefreshTimestamp(key: RefreshCacheKey): number {
  if (typeof localStorage === "undefined") {
    return 0;
  }
  let raw = localStorage.getItem(storageKey(key));
  if (!raw && key === "chatgptDesktop") {
    const legacyKey = `${STORAGE_PREFIX}codexClient`;
    raw = localStorage.getItem(legacyKey);
    if (raw) {
      localStorage.setItem(storageKey(key), raw);
      localStorage.removeItem(legacyKey);
    }
  }
  if (!raw) {
    return 0;
  }
  const value = Number(raw);
  return Number.isFinite(value) && value > 0 ? value : 0;
}

export function writeRefreshTimestamp(key: RefreshCacheKey, value = Date.now()) {
  if (typeof localStorage === "undefined") {
    return;
  }
  localStorage.setItem(storageKey(key), String(value));
}

export function refreshTimestampFresh(key: RefreshCacheKey, ttlMs = REFRESH_CACHE_TTL_MS) {
  const timestamp = readRefreshTimestamp(key);
  return timestamp > 0 && Date.now() - timestamp <= ttlMs;
}
