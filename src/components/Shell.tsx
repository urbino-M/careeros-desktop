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
  careerSystem: CareerSystem;
  locale: Locale;
  onCareerSystem: (system: CareerSystem, destination?: AppRoute) => void;
  onLocale: (locale: Locale) => void;
  onNavigate: (route: AppRoute) => void;
  children: React.ReactNode;
}

function isActive(route: AppRoute, target: AppRoute) {
  if (target.page === "applications") {
    return route.page === "applications" || route.page === "application";
  }
  return route.page === target.page;
}

export function Shell({
  route,
  careerSystem,
  locale,
  onCareerSystem,
  onLocale,
  onNavigate,
  children,
}: ShellProps) {
  const internship = careerSystem === "internship";
  const navItems: Array<{ label: string; icon: typeof Home; route: AppRoute }> = [
    { label: "仪表盘", icon: Home, route: { page: "dashboard" } },
    { label: internship ? "机会搜索" : "Agent 运行中心", icon: Bot, route: { page: "automation" } },
    { label: "设置", icon: Settings, route: { page: "settings" } },
  ];
  const applicationItems: Array<{
    label: string;
    system: CareerSystem;
    icon: typeof Home;
    route: Extract<AppRoute, { page: "applications" }>;
  }> = [
    { label: "Postdoc 申请", system: "postdoc", icon: BriefcaseBusiness, route: { page: "applications", status: "ready_to_contact" } },
    { label: "Internship 申请", system: "internship", icon: BriefcaseBusiness, route: { page: "applications", status: "all" } },
  ];
  const startDragging = (event: React.MouseEvent<HTMLElement>) => {
    if (event.button === 0) void getCurrentWindow().startDragging();
  };
  return (
    <div className="app-shell">
      <div className="window-titlebar" data-tauri-drag-region onMouseDown={startDragging}>
        <span data-tauri-drag-region>{internship ? "INTERNOS" : "POSTDOCOS"} · LOCAL WORKSPACE</span>
      </div>
      <aside className="sidebar">
        <div className="traffic-spacer" data-tauri-drag-region onMouseDown={startDragging} />
        <div className="brand-lockup">
          <div className="brand-mark">{internship ? "I" : "P"}</div>
          <div>
            <div className="brand-kicker">{internship ? "行业实习决策系统" : "研究机会决策系统"}</div>
            <div className="brand-name">{internship ? "InternOS" : "PostdocOS"}</div>
          </div>
        </div>

        <div className="workspace-switcher" role="group" aria-label="选择职业系统">
          <span>选择系统</span>
          <div>
            <button className={!internship ? "selected" : ""} aria-pressed={!internship} onClick={() => onCareerSystem("postdoc")}>PostdocOS</button>
            <button className={internship ? "selected" : ""} aria-pressed={internship} onClick={() => onCareerSystem("internship")}>InternOS</button>
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
              active={careerSystem === item.system && isActive(route, item.route)}
              onNavigate={(destination) => onCareerSystem(item.system, destination)}
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
