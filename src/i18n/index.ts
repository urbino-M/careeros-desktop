import { getUiPreferences } from "../uiPreferences";
import type { Locale } from "../types";
import { commonMessages } from "./common";
import { settingsMessages } from "./settings";
import { onboardingMessages } from "./onboarding";
import { applicationsMessages } from "./applications";
import { detailMessages } from "./detail";
import { automationMessages } from "./automation";
import { dashboardMessages } from "./dashboard";
import { updatesMessages } from "./updates";
import { internshipMessages } from "./internship";

// Only explicitly marked UI copy is translated. User input, research evidence,
// CVs, mail bodies and provider identifiers stay in their original language.
const messages: Record<string, string> = { ...commonMessages, ...settingsMessages, ...onboardingMessages, ...applicationsMessages, ...detailMessages, ...automationMessages, ...dashboardMessages, ...updatesMessages, ...internshipMessages };

export function translate(source: string, locale: Locale, ...values: Array<string | number>): string {
  const message = locale === "en" ? messages[source] ?? (messages[source.trim()] ? source.replace(source.trim(), messages[source.trim()]) : source) : source;
  return message.replace(/\{(\d+)\}/g, (token, index: string) => values[Number(index)] === undefined ? token : String(values[Number(index)]));
}

export function t(source: string, ...values: Array<string | number>): string {
  return translate(source, getUiPreferences().locale, ...values);
}

export function dateLocale(): string { return getUiPreferences().locale === "en" ? "en-US" : "zh-CN"; }
