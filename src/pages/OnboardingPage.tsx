import { t } from "../i18n";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowRight, Bot, BriefcaseBusiness, ExternalLink,
  FileUp, Globe2, GraduationCap, KeyRound, Languages, Mail, SearchCheck, Sparkles,
} from "lucide-react";
import { useState } from "react";
import { api, errorMessage } from "../api";
import type { OnboardingProfile } from "../types";
import { CareerOSMark } from "../components/CareerOSBrand";
import { InterfacePreferences } from "../components/InterfacePreferences";
import { uiNotice, useUiNotice } from "../components/Ui";

const stages = [
  ["undergraduate", "本科 / 本科毕业"], ["masters", "硕士阶段"], ["doctoral", "博士阶段"],
  ["postdoctoral", "博士后 / 青年研究者"], ["research_staff", "研究机构人员"],
  ["faculty", "高校教师"], ["industry", "产业界"], ["career_transition", "转型或跨领域"], ["other", "其他阶段"],
] as const;

const disciplines = [
  ["humanities_arts", "人文与艺术"], ["social_sciences", "社会科学"],
  ["natural_sciences", "自然科学"], ["engineering_technology", "工程与技术"],
  ["medical_life_sciences", "医学与生命科学"], ["interdisciplinary", "跨学科"], ["other", "其他"],
] as const;

export function OnboardingPage({ initial, onComplete }: { initial: OnboardingProfile; onComplete: (value: OnboardingProfile) => void }) {
  const [draft, setDraft] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useUiNotice();
  const [providerUrl, setProviderUrl] = useState("");
  const [providerKey, setProviderKey] = useState("");
  const [chatGptConnected, setChatGptConnected] = useState(false);
  const [providerConnected, setProviderConnected] = useState(false);
  const [gmailConfigured, setGmailConfigured] = useState(false);

  const patch = (value: Partial<OnboardingProfile>) => setDraft((current) => ({ ...current, ...value }));
  const persist = async (nextStep: number, completed = false) => {
    setBusy(true); setNotice("");
    try {
      const saved = await api.saveOnboardingProfile({ ...draft, currentStep: nextStep, completed });
      setDraft(saved);
      if (completed) onComplete(saved);

    } catch (value) { setNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };

  const importCv = async () => {
    const selected = await openDialog({ multiple: false, directory: false, filters: [{ name: "CV", extensions: ["pdf", "docx", "md", "txt"] }] });
    if (!selected) return;
    setBusy(true); setNotice("");
    try {
      const cvSourceFile = await api.importOnboardingCv(selected);
      const saved = await api.saveOnboardingProfile({ ...draft, cvSourceFile });
      setDraft(saved);
      setNotice("CV 已导入并整理为任务资料。首次搜索会自动识别背景；原文件和已有材料不变。");
    } catch (value) { setNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };

  const connectChatGpt = async () => {
    setBusy(true); setNotice("");
    try {
      const result = await api.connectChatGpt();
      const url = findUrl(result); const loginId = findKeyString(result, "loginId");
      if (!url || !loginId) throw new Error("没有取得完整的 ChatGPT 授权地址");
      await openUrl(url);
      setNotice("浏览器授权完成后回到这里，系统会自动核验。");
      await api.waitForChatGptLogin(loginId);
      setChatGptConnected(true); setNotice("ChatGPT / Codex 已连接。");
    } catch (value) { setNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };

  const connectProvider = async () => {
    setBusy(true); setNotice("");
    try {
      const provider = await api.connectResponsesProvider({ baseUrl: providerUrl, apiKey: providerKey });
      setProviderKey(""); setProviderConnected(true);
      setNotice(uiNotice("{0} 已连接并完成 Responses 兼容性验证。", provider.displayName));
    } catch (value) { setNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };

  const importGmailJson = async () => {
    const selected = await openDialog({ multiple: false, directory: false, filters: [{ name: "Google OAuth JSON", extensions: ["json"] }] });
    if (!selected) return;
    setBusy(true); setNotice("");
    try { await api.importGmailClient(selected); setGmailConfigured(true); setNotice("Gmail 桌面 OAuth 客户端已导入；可以稍后在设置中连接账号。"); }
    catch (value) { setNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };

  return <div className="onboarding-root">
    <div className="onboarding-drag" data-tauri-drag-region>{t("CAREEROS · 从这里开始")}</div>
    <aside className="onboarding-rail">
      <CareerOSMark />
      <div><span>{t("一份 CV，即可开始")}</span><strong>{t("CV → 机会 → 材料")}</strong></div>
      <p>{t("无需填写完整档案。简历保存在本机；启动 Agent 时，相关资料会交给你选择的模型服务处理。不会自动发信或投递。")}</p>
    </aside>
    <main className="onboarding-stage">
      <div className="onboarding-interface"><InterfacePreferences /></div>
      <header><span className="eyebrow">{t("从 CV 开始")}</span><h1>{t("上传简历，就从这里开始")}</h1><p>{t("系统根据你的 CV 整理背景、寻找机会，再准备可审核的材料。")}</p></header>
      <section className="onboarding-panel cv-import-panel">
        <FileUp size={42} /><h2>{draft.cvSourceFile ? t("CV 已导入") : t("选择一份现有 CV")}</h2><p>{draft.cvSourceFile || t("选择现有简历即可。不要求事先整理字段或逐条确认经历。")}</p>
        <button className="button primary" disabled={busy} onClick={() => void importCv()}><FileUp size={17} /> {draft.cvSourceFile ? t("更换导入文件") : t("选择 CV 文件")}</button>
        <div className="onboarding-note">{t("支持 PDF、DOCX、Markdown、TXT，最大 25 MB。更换文件不会删除此前导入的副本。")}</div>
      </section>
      <section className="onboarding-panel onboarding-form"><label><span>{t("你想寻找什么？（可稍后填写）")}</span><textarea value={draft.goals} onChange={(event) => patch({ goals: event.target.value })} placeholder={t("描述希望寻找的岗位、研究方向或地区；暂不确定也没关系。")} /></label></section>
      <details className="onboarding-optional"><summary>{t("补充或更正个人偏好（可选）")}</summary>
      <section className="onboarding-panel">
        <div className="onboarding-question"><GraduationCap size={20} /><div><strong>{t("当前阶段")}</strong><span>{t("选择最接近的即可，之后可以修改。")}</span></div></div>
        <div className="choice-grid stage-grid">{stages.map(([value, label]) => <button className={draft.careerStage === value ? "selected" : ""} onClick={() => patch({ careerStage: value })} key={value}>{t(label)}</button>)}</div>
        <div className="onboarding-question"><Globe2 size={20} /><div><strong>{t("主要学科")}</strong><span>{t("学科只用于调整证据与表达方式，不限制跨学科机会。")}</span></div></div>
        <div className="choice-grid">{disciplines.map(([value, label]) => <button className={draft.discipline === value ? "selected" : ""} onClick={() => patch({ discipline: value })} key={value}>{t(label)}</button>)}</div>
      </section>
      <section className="onboarding-panel onboarding-form">
        <div className="two-fields"><label><span>{t("姓名")}</span><input value={draft.fullName} onChange={(event) => patch({ fullName: event.target.value })} placeholder={t("用于材料页眉；留空时以导入资料为准")} /></label><label><span>{t("论文或成果中的署名形式")}</span><input value={draft.publicationName} onChange={(event) => patch({ publicationName: event.target.value })} placeholder={t("用于在作者列表中识别并加粗本人姓名")} /></label></div>
        <label><span>{t("当前情况")}</span><textarea value={draft.currentSituation} onChange={(event) => patch({ currentSituation: event.target.value })} placeholder={t("简要说明当前身份、研究或工作背景，以及正在发生的变化。")} /></label>
        <div className="two-fields"><label><span>{t("目标岗位或机会类型")}</span><input value={draft.targetRoles} onChange={(event) => patch({ targetRoles: event.target.value })} placeholder={t("可以填写多个方向或暂时保持开放")} /></label><label><span>{t("目标地区")}</span><input value={draft.targetRegions} onChange={(event) => patch({ targetRegions: event.target.value })} placeholder={t("国家、地区、城市，或远程/不限")} /></label></div>
        <label><span>{t("这次最重要的目标")}</span><textarea value={draft.goals} onChange={(event) => patch({ goals: event.target.value })} placeholder={t("说明你希望系统优先解决的问题和判断标准。")} /></label>
        <label><span>{t("现实约束")}</span><textarea value={draft.constraints} onChange={(event) => patch({ constraints: event.target.value })} placeholder={t("时间、资格、语言、地点、签证、家庭或其他必须尊重的边界。")} /></label>
        <label><span>{t("材料语言")}</span><select value={draft.preferredLanguage} onChange={(event) => patch({ preferredLanguage: event.target.value as OnboardingProfile["preferredLanguage"] })}><option value="bilingual">{t("中英双语")}</option><option value="zh">{t("中文优先")}</option><option value="en">{t("英文优先")}</option></select></label>
      </section>
      </details>
      <details className="onboarding-optional"><summary>{t("连接模型服务（首次搜索前需要）· Gmail 可选")}</summary><section className="onboarding-panel auth-grid">
        <article><Bot size={24} /><h3>ChatGPT / Codex</h3><p>{t("通过系统浏览器授权，不在应用内输入 ChatGPT 密码。")}</p><button className="button primary" disabled={busy || chatGptConnected} onClick={() => void connectChatGpt()}>{chatGptConnected ? t("已连接") : t("连接 ChatGPT")}</button></article>
        <article><KeyRound size={24} /><h3>{t("模型 URL + API Key")}</h3><p>{t("适用于兼容 OpenAI Responses 的中转站或模型服务；密钥保存在当前用户的 CareerOS 私有凭据文件中。")}</p><input aria-label={t("服务地址")} value={providerUrl} onChange={(event) => setProviderUrl(event.target.value)} placeholder="https://provider.example/v1" /><input aria-label={t("API 密钥")} type="password" value={providerKey} onChange={(event) => setProviderKey(event.target.value)} placeholder={t("API 密钥")} /><button className="button secondary" disabled={busy || providerConnected || !providerUrl.trim() || providerKey.trim().length < 8} onClick={() => void connectProvider()}>{providerConnected ? t("已连接") : t("验证并连接")}</button></article>
        <article><Mail size={24} /><h3>{t("Gmail 草稿（可选）")}</h3><p>{t("先按 Google 官方步骤创建“桌面应用”OAuth 客户端并下载 JSON。")}</p><button className="button ghost" onClick={() => openUrl("https://developers.google.com/workspace/gmail/api/quickstart/nodejs")}><ExternalLink size={15} /> {t(" 打开官方教程")}</button><button className="button secondary" disabled={busy || gmailConfigured} onClick={() => void importGmailJson()}>{gmailConfigured ? t("JSON 已导入") : t("选择客户端 JSON")}</button></article>
      </section></details>
      <details className="onboarding-optional"><summary>{t("常用功能与使用说明")}</summary><section className="onboarding-panel feature-tour">
        <article><SearchCheck /><div><h3>{t("完整检索")}</h3><p>{t("按阶段、学科、目标与约束寻找并核验机会。")}</p></div></article>
        <article><Sparkles /><div><h3>{t("材料定制")}</h3><p>{t("根据具体机会整理 CV、联系信和匹配分析。")}</p></div></article>
        <article><BriefcaseBusiness /><div><h3>{t("申请与联系人")}</h3><p>{t("每个联系人独立管理状态、投递标记、材料和版本。")}</p></div></article>
        <article><Mail /><div><h3>{t("回复处理")}</h3><p>{t("记录回复、分析真实意图并准备后续动作，不自动发送。")}</p></div></article>
        <article><Languages /><div><h3>{t("跨学科表达")}</h3><p>{t("人文社科、自然科学、工程技术与跨学科使用各自合适的证据语言。")}</p></div></article>
      </section></details>
      {notice && <div className="onboarding-notice">{notice}</div>}
      <footer>
        <span>{t("资料缺项时只询问必要信息，不阻止先查看机会。")}</span>
        <button className="button primary" disabled={busy} onClick={() => void persist(0, true)}>{busy ? t("正在整理，请稍候…") : draft.cvSourceFile ? t("保存并开始使用") : t("先进入，稍后上传")} <ArrowRight size={16} /></button>
      </footer>
    </main>
  </div>;
}

function findUrl(value: unknown): string | undefined {
  if (typeof value === "string" && /^https?:\/\//.test(value)) return value;
  if (Array.isArray(value)) { for (const item of value) { const result = findUrl(item); if (result) return result; } }
  if (value && typeof value === "object") { for (const item of Object.values(value)) { const result = findUrl(item); if (result) return result; } }
  return undefined;
}

function findKeyString(value: unknown, key: string): string | undefined {
  if (Array.isArray(value)) { for (const item of value) { const result = findKeyString(item, key); if (result) return result; } }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if (typeof record[key] === "string") return record[key] as string;
    for (const item of Object.values(record)) { const result = findKeyString(item, key); if (result) return result; }
  }
  return undefined;
}
