import {
  Bot,
  BriefcaseBusiness,
  Home,
  Settings,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppRoute, CareerSystem } from "../types";
import { CareerOSMark } from "./CareerOSBrand";
import { InterfacePreferences } from "./InterfacePreferences";
import { t } from "../i18n";

interface ShellProps {
  route: AppRoute;
  onNavigate: (route: AppRoute) => void;
  children: React.ReactNode;
}

function isActive(route: AppRoute, target: AppRoute, system?: CareerSystem) {
  if (target.page === "applications") {
    return (route.page === "applications" || route.page === "application") && route.careerSystem === system;
  }
  return route.page === target.page;
}

export function Shell({
  route,
  onNavigate,
  children,
}: ShellProps) {
  const navItems: Array<{ label: string; icon: typeof Home; route: AppRoute }> = [
    { label: "仪表盘", icon: Home, route: { page: "dashboard" } },
    { label: "Agent 运行中心", icon: Bot, route: { page: "automation" } },
    { label: "设置", icon: Settings, route: { page: "settings" } },
  ];
  const applicationItems: Array<{
    label: string;
    system: CareerSystem;
    icon: typeof Home;
    route: Extract<AppRoute, { page: "applications" }>;
  }> = [
    { label: "Postdoc 申请", system: "postdoc", icon: BriefcaseBusiness, route: { page: "applications", careerSystem: "postdoc", status: "ready_to_contact" } },
    { label: "Internship 申请", system: "internship", icon: BriefcaseBusiness, route: { page: "applications", careerSystem: "internship", status: "all" } },
  ];
  const startDragging = (event: React.MouseEvent<HTMLElement>) => {
    if (event.button === 0) void getCurrentWindow().startDragging();
  };
  return (
    <div className="app-shell">
      <div className="window-titlebar" data-tauri-drag-region onMouseDown={startDragging}>
        <span data-tauri-drag-region>CAREEROS · LOCAL WORKSPACE</span>
      </div>
      <aside className="sidebar">
        <div className="traffic-spacer" data-tauri-drag-region onMouseDown={startDragging} />
        <div className="brand-lockup">
          <CareerOSMark />
          <div>
            <div className="brand-kicker">{t("研究与职业机会决策系统")}</div>
            <div className="brand-name"><span>Career</span><em>OS</em></div>
          </div>
        </div>

        <nav className="primary-nav" aria-label={t("主导航")}>
          <div className="nav-heading">{t("工作台")}</div>
          {navItems.slice(0, 2).map((item) => (
            <NavButton key={item.label} {...item} active={isActive(route, item.route)} onNavigate={onNavigate} />
          ))}
          <div className="nav-heading">{t("申请")}</div>
          {applicationItems.map((item) => (
            <NavButton
              key={item.label}
              label={item.label}
              icon={item.icon}
              route={item.route}
              active={isActive(route, item.route, item.system)}
              onNavigate={onNavigate}
            />
          ))}
          <div className="nav-heading">{t("系统")}</div>
          <NavButton {...navItems[2]} active={isActive(route, navItems[2].route)} onNavigate={onNavigate} />
        </nav>

        <div className="sidebar-footer">
          <InterfacePreferences compact />
          <div className="privacy-note">{t("本机数据 · 外部操作需确认")}</div>
        </div>
      </aside>
      <main className="main-stage">
        <div className="window-drag" />
        {children}
      </main>
    </div>
  );
}

function NavButton({
  label,
  icon: Icon,
  route,
  active,
  onNavigate,
}: {
  label: string;
  icon: typeof Home;
  route: AppRoute;
  active: boolean;
  onNavigate: (route: AppRoute) => void;
}) {
  return (
    <button className={`nav-button ${active ? "active" : ""}`} aria-label={t(label)} title={t(label)} onClick={() => onNavigate(route)}>
      <span className={`nav-icon nav-icon-${route.page}`}><Icon size={18} strokeWidth={1.8} /></span>
      <span>{t(label)}</span>
    </button>
  );
}
