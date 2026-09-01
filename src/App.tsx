import { useCallback, useEffect, useState } from "react";
import { Shell } from "./components/Shell";
import { ApplicationDetailPage } from "./pages/ApplicationDetailPage";
import { ApplicationsPage } from "./pages/ApplicationsPage";
import { AutomationPage } from "./pages/AutomationPage";
import { DashboardPage } from "./pages/DashboardPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { ApplicationTab, AppRoute, Locale, StatusFilter } from "./types";

function parseHash(): AppRoute {
  const hash = window.location.hash.replace(/^#\/?/, "");
  const [page, value, section, origin, job] = hash.split("/");
  if (page === "automation") return { page: "automation", track: value === "internship" ? "internship" : undefined };
  if (page === "settings") return { page: "settings" };
  if (page === "application" && value) {
    const allowedTabs: ApplicationTab[] = ["cv", "cover_letter", "checklist", "email_en", "email_zh", "fit", "pi", "revision", "reply", "other"];
    return {
      page: "application",
      targetId: decodeURIComponent(value),
      tab: allowedTabs.includes(section as ApplicationTab) ? section as ApplicationTab : undefined,
      returnPage: origin === "automation" ? "automation" : origin === "internship" ? "internship" : undefined,
      jobId: job ? decodeURIComponent(job) : undefined,
    };
  }
  if (page === "applications") {
    const allowed = ["ready_to_contact", "contacted", "replied", "follow_up", "shelved", "all"];
    return {
      page: "applications",
      status: allowed.includes(value) ? (value as StatusFilter) : "ready_to_contact",
    };
  }
  return { page: "dashboard" };
}

function routeHash(route: AppRoute) {
  switch (route.page) {
    case "dashboard": return "#/dashboard";
    case "automation": return route.track === "internship" ? "#/automation/internship" : "#/automation";
    case "settings": return "#/settings";
    case "applications": return `#/applications/${route.status}`;
    case "application": return `#/application/${encodeURIComponent(route.targetId)}/${route.tab || "cv"}${route.returnPage ? `/${route.returnPage}` : route.jobId ? "/direct" : ""}${route.jobId ? `/${encodeURIComponent(route.jobId)}` : ""}`;
  }
}

export default function App() {
  const [route, setRoute] = useState<AppRoute>(() => parseHash());
  const [locale, setLocale] = useState<Locale>(() =>
    localStorage.getItem("postdocos-locale") === "en" ? "en" : "zh",
  );

  useEffect(() => {
    const onHash = () => setRoute(parseHash());
    window.addEventListener("hashchange", onHash);
    if (!window.location.hash) window.location.hash = "/dashboard";
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const navigate = useCallback((next: AppRoute) => {
    const nextHash = routeHash(next);
    if (window.location.hash === nextHash) setRoute(next);
    else window.location.hash = nextHash;
  }, []);

  const changeLocale = (next: Locale) => {
    setLocale(next);
    localStorage.setItem("postdocos-locale", next);
  };

  return (
    <Shell route={route} locale={locale} onLocale={changeLocale} onNavigate={navigate}>
      {route.page === "dashboard" && <DashboardPage onNavigate={navigate} />}
      {route.page === "automation" && <AutomationPage internshipMode={route.track === "internship"} onNavigate={navigate} />}
      {route.page === "applications" && (
        <ApplicationsPage status={route.status} onNavigate={navigate} />
      )}
      {route.page === "application" && (
        <ApplicationDetailPage targetId={route.targetId} initialTab={route.tab} returnPage={route.returnPage} focusJobId={route.jobId} locale={locale} onNavigate={navigate} />
      )}
      {route.page === "settings" && <SettingsPage />}
    </Shell>
  );
}
