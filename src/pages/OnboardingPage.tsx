import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft, ArrowRight, Bot, BriefcaseBusiness, Check, ExternalLink,
  FileUp, Globe2, GraduationCap, KeyRound, Languages, Mail, SearchCheck, Sparkles,
} from "lucide-react";
import { useState } from "react";
import { api, errorMessage } from "../api";
import type { OnboardingProfile } from "../types";
import { CareerOSMark } from "../components/CareerOSBrand";

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

const stepCopy = [
  ["你现在处于什么阶段？", "阶段与学科决定机会类型、评价证据和材料语言，不预设任何专业。"],
  ["你想去哪里？", "告诉系统你的现状、目标和现实约束，Agent 会把它们作为筛选边界。"],
  ["把现有 CV 带进来", "保留原文件，只在本机资料目录建立一份导入副本；支持 PDF、DOCX、Markdown 和纯文本。"],
  ["连接你使用的服务", "可以使用 ChatGPT / Codex OAuth，或填写兼容 Responses 的 URL 与 API Key；Gmail 完全可选。"],
  ["准备好了", "这些常用入口围绕同一份本地资料工作，外部发送与提交仍需你确认。"],
] as const;

export function OnboardingPage({ initial, onComplete }: { initial: OnboardingProfile; onComplete: (value: OnboardingProfile) => void }) {
  const [draft, setDraft] = useState(initial);
  const [step, setStep] = useState(Math.min(initial.currentStep, 4));
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
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
      else setStep(nextStep);
    } catch (value) { setNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };
  const nextDisabled = step === 0 && (draft.careerStage === "not_specified" || draft.discipline === "not_specified");

  const importCv = async () => {
    const selected = await openDialog({ multiple: false, directory: false, filters: [{ name: "CV", extensions: ["pdf", "docx", "md", "txt"] }] });
    if (!selected) return;
    setBusy(true); setNotice("");
    try {
      const cvSourceFile = await api.importOnboardingCv(selected);
      patch({ cvSourceFile });
      setNotice("CV 已复制到本机用户资料目录；原文件未改动。");
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
      setNotice(`${provider.displayName} 已连接并完成 Responses 兼容性验证。`);
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
    <div className="onboarding-drag" data-tauri-drag-region>CAREEROS · START HERE</div>
    <aside className="onboarding-rail">
      <CareerOSMark />
      <div><span>开始使用</span><strong>{String(step + 1).padStart(2, "0")} / 05</strong></div>
      <ol>{stepCopy.map(([title], index) => <li className={index === step ? "active" : index < step ? "done" : ""} key={title}><i>{index < step ? <Check size={13} /> : index + 1}</i><span>{title}</span></li>)}</ol>
      <p>所有资料保存在本机；外部操作始终需要确认。</p>
    </aside>
    <main className="onboarding-stage">
      <header><span className="eyebrow">A CONVERSATION, NOT A FORM</span><h1>{stepCopy[step][0]}</h1><p>{stepCopy[step][1]}</p></header>
      {step === 0 && <section className="onboarding-panel">
        <div className="onboarding-question"><GraduationCap size={20} /><div><strong>当前阶段</strong><span>选择最接近的即可，之后可以修改。</span></div></div>
        <div className="choice-grid stage-grid">{stages.map(([value, label]) => <button className={draft.careerStage === value ? "selected" : ""} onClick={() => patch({ careerStage: value })} key={value}>{label}</button>)}</div>
        <div className="onboarding-question"><Globe2 size={20} /><div><strong>主要学科</strong><span>学科只用于调整证据与表达方式，不限制跨学科机会。</span></div></div>
        <div className="choice-grid">{disciplines.map(([value, label]) => <button className={draft.discipline === value ? "selected" : ""} onClick={() => patch({ discipline: value })} key={value}>{label}</button>)}</div>
      </section>}
      {step === 1 && <section className="onboarding-panel onboarding-form">
        <div className="two-fields"><label><span>姓名</span><input value={draft.fullName} onChange={(event) => patch({ fullName: event.target.value })} placeholder="用于材料页眉；留空时以导入资料为准" /></label><label><span>论文或成果中的署名形式</span><input value={draft.publicationName} onChange={(event) => patch({ publicationName: event.target.value })} placeholder="用于在作者列表中识别并加粗本人姓名" /></label></div>
        <label><span>当前情况</span><textarea value={draft.currentSituation} onChange={(event) => patch({ currentSituation: event.target.value })} placeholder="简要说明当前身份、研究或工作背景，以及正在发生的变化。" /></label>
        <div className="two-fields"><label><span>目标岗位或机会类型</span><input value={draft.targetRoles} onChange={(event) => patch({ targetRoles: event.target.value })} placeholder="可以填写多个方向或暂时保持开放" /></label><label><span>目标地区</span><input value={draft.targetRegions} onChange={(event) => patch({ targetRegions: event.target.value })} placeholder="国家、地区、城市，或远程/不限" /></label></div>
        <label><span>这次最重要的目标</span><textarea value={draft.goals} onChange={(event) => patch({ goals: event.target.value })} placeholder="说明你希望系统优先解决的问题和判断标准。" /></label>
        <label><span>现实约束</span><textarea value={draft.constraints} onChange={(event) => patch({ constraints: event.target.value })} placeholder="时间、资格、语言、地点、签证、家庭或其他必须尊重的边界。" /></label>
        <label><span>材料语言</span><select value={draft.preferredLanguage} onChange={(event) => patch({ preferredLanguage: event.target.value as OnboardingProfile["preferredLanguage"] })}><option value="bilingual">中英双语</option><option value="zh">中文优先</option><option value="en">英文优先</option></select></label>
      </section>}
      {step === 2 && <section className="onboarding-panel cv-import-panel">
        <FileUp size={42} /><h2>{draft.cvSourceFile ? "CV 已导入" : "选择一份现有 CV"}</h2><p>{draft.cvSourceFile || "上传内容会成为 Agent 的资料来源，但不会自动把无法核验的描述当成事实。"}</p>
        <button className="button primary" disabled={busy} onClick={() => void importCv()}><FileUp size={17} /> {draft.cvSourceFile ? "更换导入文件" : "选择 CV 文件"}</button>
        <div className="onboarding-note">支持 PDF、DOCX、Markdown、TXT，最大 25 MB。更换文件不会删除此前导入的副本。</div>
      </section>}
      {step === 3 && <section className="onboarding-panel auth-grid">
        <article><Bot size={24} /><h3>ChatGPT / Codex</h3><p>通过系统浏览器授权，不在应用内输入 ChatGPT 密码。</p><button className="button primary" disabled={busy || chatGptConnected} onClick={() => void connectChatGpt()}>{chatGptConnected ? "已连接" : "连接 ChatGPT"}</button></article>
        <article><KeyRound size={24} /><h3>模型 URL + API Key</h3><p>适用于兼容 OpenAI Responses 的中转站或模型服务；密钥保存在当前用户的 CareerOS 私有凭据文件中。</p><input value={providerUrl} onChange={(event) => setProviderUrl(event.target.value)} placeholder="https://provider.example/v1" /><input type="password" value={providerKey} onChange={(event) => setProviderKey(event.target.value)} placeholder="API Key" /><button className="button secondary" disabled={busy || providerConnected || !providerUrl.trim() || providerKey.trim().length < 8} onClick={() => void connectProvider()}>{providerConnected ? "已连接" : "验证并连接"}</button></article>
        <article><Mail size={24} /><h3>Gmail 草稿（可选）</h3><p>先按 Google 官方步骤创建“桌面应用”OAuth 客户端并下载 JSON。</p><button className="button ghost" onClick={() => openUrl("https://developers.google.com/workspace/gmail/api/quickstart/nodejs")}><ExternalLink size={15} /> 打开官方教程</button><button className="button secondary" disabled={busy || gmailConfigured} onClick={() => void importGmailJson()}>{gmailConfigured ? "JSON 已导入" : "选择客户端 JSON"}</button></article>
      </section>}
      {step === 4 && <section className="onboarding-panel feature-tour">
        <article><SearchCheck /><div><h3>完整检索</h3><p>按阶段、学科、目标与约束寻找并核验机会。</p></div></article>
        <article><Sparkles /><div><h3>材料定制</h3><p>根据具体机会整理 CV、联系信和匹配分析。</p></div></article>
        <article><BriefcaseBusiness /><div><h3>申请与联系人</h3><p>每个联系人独立管理状态、投递标记、材料和版本。</p></div></article>
        <article><Mail /><div><h3>回复处理</h3><p>记录回复、分析真实意图并准备后续动作，不自动发送。</p></div></article>
        <article><Languages /><div><h3>跨学科表达</h3><p>人文社科、自然科学、工程技术与跨学科使用各自合适的证据语言。</p></div></article>
      </section>}
      {notice && <div className="onboarding-notice">{notice}</div>}
      <footer>
        <button className="button ghost" disabled={busy || step === 0} onClick={() => setStep((value) => Math.max(0, value - 1))}><ArrowLeft size={16} /> 上一步</button>
        <div><button className="onboarding-skip" disabled={busy} onClick={() => void persist(step, true)}>稍后补充，进入应用</button>{step < 4 ? <button className="button primary" disabled={busy || nextDisabled} onClick={() => void persist(step + 1)}>{busy ? "保存中…" : "继续"} <ArrowRight size={16} /></button> : <button className="button primary" disabled={busy} onClick={() => void persist(5, true)}>{busy ? "保存中…" : "进入工作台"} <ArrowRight size={16} /></button>}</div>
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
