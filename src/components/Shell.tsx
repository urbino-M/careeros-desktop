import {
  Bot,
  BriefcaseBusiness,
  Home,
  Languages,
  Settings,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppRoute, Locale } from "../types";

interface ShellProps {
  route: AppRoute;
  locale: Locale;
  onLocale: (locale: Locale) => void;
  onNavigate: (route: AppRoute) => void;
  children: React.ReactNode;
}

const navItems: Array<{
  page: AppRoute["page"];
  label: string;
  icon: typeof Home;
  route: AppRoute;
}> = [
  { page: "dashboard", label: "仪表盘", icon: Home, route: { page: "dashboard" } },
  { page: "automation", label: "Agent 运行中心", icon: Bot, route: { page: "automation" } },
  {
    page: "applications",
    label: "申请中心",
    icon: BriefcaseBusiness,
    route: { page: "applications", status: "ready_to_contact" },
  },
  { page: "settings", label: "设置", icon: Settings, route: { page: "settings" } },
];

function isActive(route: AppRoute, page: AppRoute["page"]) {
  if (page === "applications") {
    return route.page === "applications" || route.page === "application";
  }
  return route.page === page;
}

export function Shell({
  route,
  locale,
  onLocale,
  onNavigate,
  children,
}: ShellProps) {
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
            <div className="brand-kicker">研究机会决策系统</div>
            <div className="brand-name">PostdocOS</div>
          </div>
        </div>

        <nav className="primary-nav" aria-label="主导航">
          <div className="nav-heading">工作台</div>
          {navItems.slice(0, 2).map((item) => (
            <NavButton key={item.page} {...item} active={isActive(route, item.page)} onNavigate={onNavigate} />
          ))}
          <div className="nav-heading">申请</div>
          <NavButton {...navItems[2]} active={isActive(route, "applications")} onNavigate={onNavigate} />
          <div className="nav-heading">系统</div>
          <NavButton {...navItems[3]} active={isActive(route, "settings")} onNavigate={onNavigate} />
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
