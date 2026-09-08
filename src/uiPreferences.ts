import { useSyncExternalStore } from "react";
import type { Locale } from "./types";

export type Theme = "light" | "dark";
type UiPreferences = { locale: Locale; theme: Theme };
const listeners = new Set<() => void>();

function readPreference(key: string): string | null {
  try { return globalThis.localStorage?.getItem(key) ?? null; } catch { return null; }
}

let preferences: UiPreferences = {
  locale: readPreference("careeros-locale") === "en" ? "en" : "zh",
  theme: readPreference("careeros-theme") === "light" ? "light" : "dark",
};

export function getUiPreferences(): UiPreferences { return preferences; }

export function applyUiPreferences(): void {
  if (typeof document === "undefined") return;
  document.documentElement.lang = preferences.locale === "en" ? "en" : "zh-CN";
  document.documentElement.dataset.theme = preferences.theme;
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", preferences.theme === "light" ? "#f4f6f3" : "#06171f");
}

export function setUiPreferences(next: Partial<UiPreferences>): void {
  preferences = { ...preferences, ...next };
  try {
    localStorage.setItem("careeros-locale", preferences.locale);
    localStorage.setItem("careeros-theme", preferences.theme);
  } catch { /* Restricted storage must not disable the controls in this session. */ }
  applyUiPreferences();
  listeners.forEach((notify) => notify());
}

export function useUiPreferences(): UiPreferences {
  return useSyncExternalStore((notify) => {
    listeners.add(notify);
    return () => { listeners.delete(notify); };
  }, getUiPreferences, getUiPreferences);
}

applyUiPreferences();
