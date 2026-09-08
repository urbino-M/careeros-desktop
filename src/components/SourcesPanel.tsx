import { ExternalLink } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { errorMessage } from "../api";
import { t } from "../i18n";
import { formatLocalTime, searchChannelLabels } from "./Ui";
import type { SourceEvidence } from "../types";

export function SourcesPanel({ sources = [] }: { sources?: SourceEvidence[] }) {
  const [error,setError]=useState("");
  return <section className="source-evidence-panel">
    <div className="content-title"><ExternalLink size={21} /><div><h3>{t("来源与核验证据")}</h3><p>{t("公开线索不等于官方核验；研究方向证据不等于正在招聘。")}</p></div></div>
    {sources.length === 0 ? <p className="muted-copy">{t("当前没有可展示的来源证据。")}</p> : <div className="source-evidence-list">{sources.map((source,index)=><article key={`${source.url}-${index}`}>
      <div><strong>{source.title}</strong><span>{t(searchChannelLabels[source.channel] || source.channel)} · {source.backend} · {source.evidenceType}</span></div>
      <a href={source.url} onClick={event=>{event.preventDefault();setError("");if(/^https?:\/\//i.test(source.url))void openUrl(source.url).catch(e=>setError(errorMessage(e)));}}>{source.url}<ExternalLink size={12}/></a>
      <small>{t("检查时间：")}{formatLocalTime(source.checkedAt)}</small>
    </article>)}</div>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
