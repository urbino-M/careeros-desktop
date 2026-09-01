import { useCallback, useEffect, useState } from "react";
import { Shell } from "./components/Shell";
import { ApplicationDetailPage } from "./pages/ApplicationDetailPage";
import { ApplicationsPage } from "./pages/ApplicationsPage";
import { AutomationPage } from "./pages/AutomationPage";
import { DashboardPage } from "./pages/DashboardPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { ApplicationFilter, ApplicationTab, AppRoute, CareerSystem, Locale } from "./types";

function parseHash(): AppRoute {
  const hash = window.location.hash.replace(/^#\/?/, "");
  const [page, value, section, origin, job] = hash.split("/");
  if (page === "automation") return { page: "automation" };
  if (page === "settings") return { page: "settings" };
  if (page === "application" && value) {
    const allowedTabs: ApplicationTab[] = ["cv", "cover_letter", "checklist", "email_en", "email_zh", "fit", "pi", "revision", "reply", "other"];
    return {
      page: "application",
      targetId: decodeURIComponent(value),
      tab: allowedTabs.includes(section as ApplicationTab) ? section as ApplicationTab : undefined,
      returnPage: origin === "automation" ? "automation" : undefined,
      jobId: job ? decodeURIComponent(job) : undefined,
    };
  }
  if (page === "applications") {
    const allowed = ["ready_to_contact", "contacted", "replied", "follow_up", "shelved", "not_set", "portal_pending", "submitted", "not_required", "all"];
    return {
      page: "applications",
      status: allowed.includes(value) ? (value as ApplicationFilter) : "ready_to_contact",
    };
  }
  return { page: "dashboard" };
}

function routeHash(route: AppRoute) {
  switch (route.page) {
    case "dashboard": return "#/dashboard";
    case "automation": return "#/automation";
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
  const [careerSystem, setCareerSystem] = useState<CareerSystem>(() =>
    localStorage.getItem("postdocos-career-system") === "internship" ? "internship" : "postdoc",
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

  const changeCareerSystem = (next: CareerSystem, destination: AppRoute = { page: "dashboard" }) => {
    setCareerSystem(next);
    localStorage.setItem("postdocos-career-system", next);
    navigate(destination);
  };

  return (
    <Shell route={route} careerSystem={careerSystem} locale={locale} onCareerSystem={changeCareerSystem} onLocale={changeLocale} onNavigate={navigate}>
      {route.page === "dashboard" && <DashboardPage careerSystem={careerSystem} onNavigate={navigate} />}
      {route.page === "automation" && <AutomationPage careerSystem={careerSystem} onNavigate={navigate} />}
      {route.page === "applications" && (
        <ApplicationsPage careerSystem={careerSystem} status={route.status} onNavigate={navigate} />
      )}
      {route.page === "application" && (
        <ApplicationDetailPage targetId={route.targetId} initialTab={route.tab} returnPage={route.returnPage} focusJobId={route.jobId} locale={locale} onNavigate={navigate} />
      )}
      {route.page === "settings" && <SettingsPage />}
    </Shell>
  );
}
