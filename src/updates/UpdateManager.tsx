import { dateLocale, t } from "../i18n";
import { useUiPreferences } from "../uiPreferences";
import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, Download, ExternalLink, RefreshCw, ShieldCheck, Sparkles, X } from "lucide-react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import { UPDATE_CHECK_INTERVAL_MS, UPDATE_CHECK_TIMEOUT_MS, formatLastChecked, formatUpdateMessage, updateErrorKey, updateProgress, type UpdateMessage } from "./updateCore";

const RELEASE_URL = "https://github.com/urbino-M/careeros-desktop/releases/latest";
const SKIPPED_VERSION_KEY = "careeros.updates.skipped-version";
const LAST_CHECKED_KEY = "careeros.updates.last-checked";

type UpdatePhase = "idle" | "checking" | "available" | "downloading" | "installing" | "current" | "error";

interface AvailableUpdate {
  version: string;
  currentVersion: string;
  date?: string;
  notes?: string;
}

interface UpdateContextValue {
  currentVersion: string;
  lastChecked?: string;
  lastCheckedLabel: string;
  phase: UpdatePhase;
  message: string;
  available?: AvailableUpdate;
  checkNow: () => Promise<void>;
}

const UpdateContext = createContext<UpdateContextValue | undefined>(undefined);

export function useDesktopUpdates() {
  const value = useContext(UpdateContext);
  if (!value) throw new Error("useDesktopUpdates must be used inside UpdateManager");
  return value;
}

export function UpdateManager({ children }: { children: React.ReactNode }) {
  const { locale } = useUiPreferences();
  const updateRef = useRef<Update | null>(null);
  const downloadedRef = useRef(false);
  const [currentVersion, setCurrentVersion] = useState("—");
  const [lastChecked, setLastChecked] = useState<string | undefined>(() => localStorage.getItem(LAST_CHECKED_KEY) || undefined);
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [message, setMessage] = useState<UpdateMessage>("启动后会自动检查 GitHub Release");
  const [available, setAvailable] = useState<AvailableUpdate>();
  const [visible, setVisible] = useState(false);
  const [downloaded, setDownloaded] = useState(0);
  const [downloadTotal, setDownloadTotal] = useState<number>();

  const releaseUpdate = useCallback(async () => {
    const active = updateRef.current;
    updateRef.current = null;
    downloadedRef.current = false;
    if (active) await active.close().catch(() => undefined);
  }, []);

  const checkForUpdate = useCallback(async (manual: boolean) => {
    if (!isTauri()) {
      setMessage("更新检查仅在安装后的桌面应用中可用");
      return;
    }
    setPhase("checking");
    setMessage(manual ? "正在检查 GitHub Release…" : "正在后台检查更新…");
    try {
      await releaseUpdate();
      const result = await check({ timeout: UPDATE_CHECK_TIMEOUT_MS });
      const checkedAt = new Date().toISOString();
      localStorage.setItem(LAST_CHECKED_KEY, checkedAt);
      setLastChecked(checkedAt);
      if (!result) {
        setAvailable(undefined);
        setPhase("current");
        setMessage("当前已是最新版本");
        return;
      }
      const metadata = {
        version: result.version,
        currentVersion: result.currentVersion,
        date: result.date,
        notes: result.body,
      };
      const skipped = localStorage.getItem(SKIPPED_VERSION_KEY) === result.version;
      if (skipped && !manual) {
        await result.close();
        setAvailable(metadata);
        setPhase("idle");
        setMessage({ key: "已跳过 v{0}", values: [result.version] });
        return;
      }
      updateRef.current = result;
      downloadedRef.current = false;
      setAvailable(metadata);
      setPhase("available");
      setMessage({ key: "发现新版本 v{0}", values: [result.version] });
      setVisible(true);
    } catch (value) {
      setPhase("error");
      setMessage(updateErrorKey(value));
      if (manual) setVisible(false);
    }
  }, [releaseUpdate]);

  const checkNow = useCallback(() => checkForUpdate(true), [checkForUpdate]);

  useEffect(() => {
    if (!isTauri()) return;
    void getVersion().then(setCurrentVersion).catch(() => setCurrentVersion("—"));
    const isDevelopment = (import.meta as ImportMeta & { readonly env?: { readonly DEV?: boolean } }).env?.DEV;
    if (isDevelopment) return;
    const startupTimer = window.setTimeout(() => void checkForUpdate(false), 2500);
    const interval = window.setInterval(() => void checkForUpdate(false), UPDATE_CHECK_INTERVAL_MS);
    return () => {
      window.clearTimeout(startupTimer);
      window.clearInterval(interval);
    };
  }, [checkForUpdate]);

  useEffect(() => () => { void releaseUpdate(); }, [releaseUpdate]);

  const dismiss = async () => {
    if (phase === "downloading" || phase === "installing") return;
    setVisible(false);
    setPhase("idle");
    setMessage(available ? { key: "v{0} 可用，稍后可以在设置中继续", values: [available.version] } : "");
    await releaseUpdate();
  };

  const skip = async () => {
    if (!available) return;
    localStorage.setItem(SKIPPED_VERSION_KEY, available.version);
    setVisible(false);
    setPhase("idle");
    setMessage({ key: "已跳过 v{0}", values: [available.version] });
    await releaseUpdate();
  };

  const install = async () => {
    const active = updateRef.current;
    if (!active) {
      await checkForUpdate(true);
      return;
    }
    setDownloaded(0);
    setDownloadTotal(undefined);
    setPhase("downloading");
    setMessage("正在后台下载并校验更新包…");
    try {
      if (!downloadedRef.current) {
        await active.download((event) => {
          if (event.event === "Started") setDownloadTotal(event.data.contentLength);
          if (event.event === "Progress") setDownloaded((value) => value + event.data.chunkLength);
          if (event.event === "Finished") setPhase("installing");
        }, { timeout: 120_000 });
        downloadedRef.current = true;
      }
      setPhase("installing");
      setMessage("签名校验通过，正在安装更新…");
      await active.install({ restartAfterInstall: true });
      await relaunch();
    } catch (value) {
      setPhase("error");
      setMessage(updateErrorKey(value));
    }
  };

  const context = useMemo<UpdateContextValue>(() => ({
    currentVersion,
    lastChecked,
    lastCheckedLabel: formatLastChecked(lastChecked),
    phase,
    message: formatUpdateMessage(message),
    available,
    checkNow,
  }), [available, checkNow, currentVersion, lastChecked, message, phase, locale]);
  const progress = updateProgress(downloaded, downloadTotal);
  const busy = phase === "downloading" || phase === "installing";

  return (
    <UpdateContext.Provider value={context}>
      {children}
      {visible && available && (
        <div className="update-backdrop" role="presentation">
          <section className="update-dialog" role="dialog" aria-modal="true" aria-labelledby="update-title">
            <header>
              <div className="update-dialog-symbol"><Sparkles size={24} /></div>
              <div><span>{t("桌面更新")}</span><h2 id="update-title">{t("发现新版本")}</h2></div>
              <button aria-label={t("稍后更新")} disabled={busy} onClick={() => void dismiss()}><X size={21} /></button>
            </header>
            <div className="update-dialog-body">
              <div className="update-version-row">
                <div><span>{t("当前版本")}</span><strong>v{available.currentVersion}</strong></div>
                <div className="update-version-line" />
                <div><span>{t("可用版本")}</span><strong>v{available.version}</strong></div>
              </div>
              <div className="update-trust-strip"><ShieldCheck size={17} /><span>{t("安装前会强制校验 CareerOS 发布签名")}</span>{available.date && <time>{new Date(available.date).toLocaleDateString(dateLocale())}</time>}</div>
              <div className="update-notes">
                <h3>{t("更新内容")}</h3>
                {available.notes ? <ReactMarkdown components={{ a: ({ href, children: linkChildren }) => <button className="update-note-link" onClick={() => href && void openUrl(href)}>{linkChildren} <ExternalLink size={12} /></button> }}>{available.notes}</ReactMarkdown> : <p>{t("此版本没有附加更新说明。")}</p>}
              </div>
              {busy && <div className="update-progress"><div><span>{phase === "downloading" ? t("下载并校验") : t("正在安装")}</span><strong>{progress === undefined ? t("处理中") : `${progress}%`}</strong></div><div className="update-progress-track"><span style={{ width: progress === undefined ? "38%" : `${progress}%` }} /></div></div>}
              {phase === "error" && <div className="update-error">{formatUpdateMessage(message)}</div>}
            </div>
            <footer>
              <button className="button ghost" disabled={busy} onClick={() => void openUrl(RELEASE_URL)}>{t("在 GitHub 查看")}</button>
              <div>
                <button className="button secondary" disabled={busy} onClick={() => void skip()}>{t("跳过此版本")}</button>
                <button className="button secondary" disabled={busy} onClick={() => void dismiss()}>{t("稍后")}</button>
                <button className="button primary" disabled={busy} onClick={() => void install()}>{phase === "error" ? <RefreshCw size={17} /> : phase === "installing" ? <CheckCircle2 size={17} /> : <Download size={17} />}{phase === "error" ? t("重试") : busy ? t("更新中…") : t("立即更新")}</button>
              </div>
            </footer>
          </section>
        </div>
      )}
    </UpdateContext.Provider>
  );
}
