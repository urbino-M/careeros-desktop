import { Languages, Moon, Sun } from "lucide-react";
import { t } from "../i18n";
import { setUiPreferences, useUiPreferences } from "../uiPreferences";

export function InterfacePreferences({ compact = false }: { compact?: boolean }) {
  const { locale, theme } = useUiPreferences();
  return <div className={`interface-preferences ${compact ? "compact" : ""}`}>
    <div>
      <div className="language-label"><Languages size={15} /> {t("界面语言")}</div>
      <div className="segmented compact" role="group" aria-label={t("界面语言")}>
        <button type="button" lang="zh-CN" aria-pressed={locale === "zh"} className={locale === "zh" ? "selected" : ""} onClick={() => setUiPreferences({ locale: "zh" })}>中文</button>
        <button type="button" lang="en" aria-pressed={locale === "en"} className={locale === "en" ? "selected" : ""} onClick={() => setUiPreferences({ locale: "en" })}>English</button>
      </div>
    </div>
    <div>
      <div className="language-label">{theme === "light" ? <Sun size={15} /> : <Moon size={15} />} {t("外观")}</div>
      <div className="segmented compact theme-options" role="group" aria-label={t("外观")}>
        <button type="button" aria-pressed={theme === "light"} className={theme === "light" ? "selected" : ""} onClick={() => setUiPreferences({ theme: "light" })}><Sun size={15} /> {t("明亮")}</button>
        <button type="button" aria-pressed={theme === "dark"} className={theme === "dark" ? "selected" : ""} onClick={() => setUiPreferences({ theme: "dark" })}><Moon size={15} /> {t("黑暗")}</button>
      </div>
    </div>
  </div>;
}
