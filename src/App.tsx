import { useCallback, useEffect, useState } from "react";
import { Shell } from "./components/Shell";
import { ApplicationDetailPage } from "./pages/ApplicationDetailPage";
import { ApplicationsPage } from "./pages/ApplicationsPage";
import { AutomationPage } from "./pages/AutomationPage";
import { DashboardPage } from "./pages/DashboardPage";
import { OnboardingPage } from "./pages/OnboardingPage";
import { SettingsPage } from "./pages/SettingsPage";
import { api, errorMessage } from "./api";
import type { ApplicationFilter, ApplicationTab, AppRoute, CareerSystem, Locale, OnboardingProfile } from "./types";

const applicationStatuses: ApplicationFilter[] = [
  "ready_to_contact",
  "contacted",
  "replied",
  "follow_up",
  "shelved",
  "not_set",
  "portal_pending",
  "submitted",
  "not_required",
  "all",
];

function isCareerSystem(value?: string): value is CareerSystem {
  return value === "postdoc" || value === "internship";
}

function parseHash(): AppRoute {
  const hash = window.location.hash.replace(/^#\/?/, "");
  const [page, value, section, origin, job, detailJob] = hash.split("/");
  if (page === "automation") return { page: "automation" };
  if (page === "settings") return { page: "settings" };
  if (page === "application" && value) {
    const allowedTabs: ApplicationTab[] = ["cv", "cover_letter", "checklist", "email_en", "email_zh", "fit", "pi", "revision", "reply", "other"];
    const prefixed = isCareerSystem(origin);
    const careerSystem: CareerSystem = prefixed
      ? origin
      : isCareerSystem(detailJob)
        ? detailJob
        : "postdoc";
    const routeOrigin = prefixed ? job : origin;
    const routeJob = prefixed ? detailJob : job;
    return {
      page: "application",
      targetId: decodeURIComponent(value),
      careerSystem,
      tab: allowedTabs.includes(section as ApplicationTab) ? section as ApplicationTab : undefined,
      returnPage: routeOrigin === "automation" ? "automation" : undefined,
      jobId: routeJob ? decodeURIComponent(routeJob) : undefined,
    };
  }
  if (page === "applications") {
    const prefixed = isCareerSystem(value);
    const careerSystem: CareerSystem = prefixed ? value : "postdoc";
    const candidateStatus = prefixed ? section : value;
    const view = careerSystem === "internship" && candidateStatus === "strategy" ? "strategy" : undefined;
    const fallbackStatus: ApplicationFilter = careerSystem === "internship" ? "all" : "ready_to_contact";
    return {
      page: "applications",
      careerSystem,
      status: view === "strategy" ? "all" : applicationStatuses.includes(candidateStatus as ApplicationFilter)
        ? candidateStatus as ApplicationFilter
        : fallbackStatus,
      view,
    };
  }
  return { page: "dashboard" };
}

function routeHash(route: AppRoute) {
  switch (route.page) {
    case "dashboard": return "#/dashboard";
    case "automation": return "#/automation";
    case "settings": return "#/settings";
    case "applications": return route.view === "strategy"
      ? `#/applications/${route.careerSystem}/strategy`
      : `#/applications/${route.careerSystem}/${route.status}`;
    case "application": {
      const parts = ["#/application", encodeURIComponent(route.targetId), route.tab || "cv", route.careerSystem];
      if (route.returnPage) parts.push(route.returnPage);
      else if (route.jobId) parts.push("direct");
      if (route.jobId) parts.push(encodeURIComponent(route.jobId));
      return parts.join("/");
    }
  }
}

export default function App() {
  const [route, setRoute] = useState<AppRoute>(() => parseHash());
  const [locale, setLocale] = useState<Locale>(() =>
    localStorage.getItem("postdocos-locale") === "en" ? "en" : "zh",
  );
  const [onboarding, setOnboarding] = useState<OnboardingProfile>();
  const [onboardingError, setOnboardingError] = useState("");

  const loadOnboarding = useCallback(() => {
    setOnboardingError("");
    api.onboardingProfile().then(setOnboarding).catch((value) => setOnboardingError(errorMessage(value)));
  }, []);

  useEffect(() => { loadOnboarding(); }, [loadOnboarding]);

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

  if (onboardingError) {
    return <div className="onboarding-boot"><strong>无法读取开始使用资料</strong><p>{onboardingError}</p><button onClick={loadOnboarding}>重新读取</button></div>;
  }
  if (!onboarding) return <div className="onboarding-boot"><span />正在准备本机工作区…</div>;
  if (!onboarding.completed) {
    return <OnboardingPage initial={onboarding} onComplete={setOnboarding} />;
  }

  return (
    <Shell route={route} locale={locale} onLocale={changeLocale} onNavigate={navigate}>
      {route.page === "dashboard" && <DashboardPage onNavigate={navigate} />}
      {route.page === "automation" && <AutomationPage onNavigate={navigate} />}
      {route.page === "applications" && (
        <ApplicationsPage careerSystem={route.careerSystem} status={route.status} view={route.view} onNavigate={navigate} />
      )}
      {route.page === "application" && (
        <ApplicationDetailPage targetId={route.targetId} initialTab={route.tab} returnPage={route.returnPage} focusJobId={route.jobId} locale={locale} onNavigate={navigate} />
      )}
      {route.page === "settings" && <SettingsPage onRestartOnboarding={async () => {
        const saved = await api.saveOnboardingProfile({ ...onboarding, completed: false, currentStep: 0 });
        setOnboarding(saved);
      }} />}
    </Shell>
  );
}
