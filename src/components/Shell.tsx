import {
  Bot,
  BriefcaseBusiness,
  Home,
  Languages,
  Settings,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppRoute, CareerSystem, Locale } from "../types";

interface ShellProps {
  route: AppRoute;
  locale: Locale;
  onLocale: (locale: Locale) => void;
  onNavigate: (route: AppRoute) => void;
  children: React.ReactNode;
}

function isActive(route: AppRoute, target: AppRoute, system?: CareerSystem) {
  if (target.page === "applications") {
    return route.page === "applications" && route.careerSystem === system;
  }
  return route.page === target.page;
}

export function Shell({
  route,
  locale,
  onLocale,
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
        <span data-tauri-drag-region>POSTDOCOS · LOCAL WORKSPACE</span>
      </div>
      <aside className="sidebar">
        <div className="traffic-spacer" data-tauri-drag-region onMouseDown={startDragging} />
        <div className="brand-lockup">
          <div className="brand-mark">P</div>
          <div>
            <div className="brand-kicker">研究与职业机会决策系统</div>
            <div className="brand-name">PostdocOS</div>
          </div>
        </div>

        <nav className="primary-nav" aria-label="主导航">
          <div className="nav-heading">工作台</div>
          {navItems.slice(0, 2).map((item) => (
            <NavButton key={item.label} {...item} active={isActive(route, item.route)} onNavigate={onNavigate} />
          ))}
          <div className="nav-heading">申请</div>
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
          <div className="nav-heading">系统</div>
          <NavButton {...navItems[2]} active={isActive(route, navItems[2].route)} onNavigate={onNavigate} />
        </nav>

        <div className="sidebar-footer">
          <div className="language-label"><Languages size={15} /> 语言 / Language</div>
          <div className="segmented compact">
            <button className={locale === "zh" ? "selected" : ""} onClick={() => onLocale("zh")}>中文</button>
            <button className={locale === "en" ? "selected" : ""} onClick={() => onLocale("en")}>English</button>
          </div>
          <div className="privacy-note">本机数据 · 外部操作需确认</div>
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
    <button className={`nav-button ${active ? "active" : ""}`} onClick={() => onNavigate(route)}>
      <Icon size={18} strokeWidth={1.8} />
      <span>{label}</span>
    </button>
  );
}
