import { useEffect, useRef, useSyncExternalStore } from "react";
import { api, errorMessage } from "../api";
import { t } from "../i18n";
import type { ArtifactItem, RevisionResult } from "../types";

type Draft = { text: string; note: string; original: string; path: string; baseHash: string;
  loading: boolean; saving: boolean; loaded: boolean; error: string | (() => string); message: string | (() => string) };
const empty: Draft = { text:"",note:"",original:"",path:"",baseHash:"",loading:false,saving:false,loaded:false,error:"",message:"" };
// Session-only: no CV contents in localStorage, telemetry or an external service.
const drafts = new Map<string,Draft>();
const listeners = new Set<() => void>();
function subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; }
const dirty = (draft: Draft) => draft.text !== draft.original || !!draft.note;
let unloadWarningInstalled = false;
function warnBeforeUnload(event: BeforeUnloadEvent) { event.preventDefault(); event.returnValue=""; }
function update(key: string, value: Draft) {
  drafts.set(key,value);
  if (typeof window !== "undefined") {
    const shouldWarn = [...drafts.values()].some(draft => dirty(draft) || draft.saving);
    if (shouldWarn !== unloadWarningInstalled) {
      if (shouldWarn) window.addEventListener("beforeunload",warnBeforeUnload);
      else window.removeEventListener("beforeunload",warnBeforeUnload);
      unloadWarningInstalled = shouldWarn;
    }
  }
  listeners.forEach(listener => listener());
}
export function materialDraftKey(targetId: string, artifact: Pick<ArtifactItem,"artifactType"|"language">) {
  return JSON.stringify([targetId,artifact.artifactType,artifact.language]);
}
export function readMaterialDraft(key: string) { return drafts.get(key) ?? empty; }
export function materialDraftFeedback(key: string) {
  const draft = readMaterialDraft(key), notice = draft.error || draft.message;
  return typeof notice === "function" ? notice() : notice;
}
export function editMaterialDraft(key: string, patch: Partial<Pick<Draft,"text"|"note">>) {
  const draft = readMaterialDraft(key);
  if (!draft.loaded || draft.loading || draft.saving) return;
  update(key,{...draft,...patch,error:"",message:""});
}
async function hashText(text: string) {
  const bytes = await crypto.subtle.digest("SHA-256",new TextEncoder().encode(text));
  return Array.from(new Uint8Array(bytes),byte => byte.toString(16).padStart(2,"0")).join("");
}
export async function loadMaterialDraft(key: string, path: string, force = false) {
  const old = drafts.get(key) ?? empty;
  if (old.saving || (!force && (dirty(old) || old.loading || (old.loaded && old.path === path)))) return;
  const loading = {...empty,path,loading:true};
  update(key,loading);
  try {
    const text = await api.readMaterial(path), baseHash = await hashText(text);
    if (drafts.get(key) !== loading) return;
    update(key,{...empty,text,original:text,path,baseHash,loaded:true});
  } catch(error) {
    if (drafts.get(key) === loading) update(key,{...loading,loading:false,error:() => t("无法载入材料：{0}", errorMessage(error))});
  }
}

export async function saveMaterialDraft(targetId: string, artifact: ArtifactItem): Promise<boolean> {
    const key = materialDraftKey(targetId,artifact);
    const current = drafts.get(key);
    if (!current || !current.loaded || current.saving || current.loading) return false;
    if (!current.text.trim()) { update(key,{...current,error:() => t("不能保存空材料；当前输入已保留。")}); return false; }
    if (artifact.artifactType === "cv_data") {
      try { JSON.parse(current.text); }
      catch(error) { update(key,{...current,error:() => t("CV JSON 格式有误，尚未提交：{0}", errorMessage(error))}); return false; }
    }
    update(key,{...current,saving:true,error:"",message:""});
    let result: RevisionResult;
    try {
      result = await api.saveManualMaterial({targetId,artifactType:artifact.artifactType,language:artifact.language,
        content:current.text,note:current.note || undefined,expectedBaseSha256:current.baseHash});
    } catch(error) {
      update(key,{...current,saving:false,error:() => t("保存未成功：{0}。输入已保留，正式版本未替换。", errorMessage(error))});
      return false;
    }
    // The command succeeded. A subsequent read failure is NOT a failed save.
    let text=current.text, baseHash=current.baseHash, readWarning=() => "", loaded=true;
    try { text=await api.readMaterial(result.artifactPath); baseHash=await hashText(text); }
    catch { readWarning=() => t("；新文件暂时无法重新读取，请重试载入后继续编辑"); loaded=false; }
    update(key,{...empty,text,original:text,path:result.artifactPath,baseHash,loaded,
      message:() => t("已保存新版本：{0}{1}。{2}", result.summary, readWarning(), artifact.artifactType === "cv_data" ? t("CV 数据和 PDF 已同步更新。") : t("历史版本已保留。"))});
    return true;
}

export function ManualMaterialEditor({targetId,artifact,onChanged}: {
  targetId:string; artifact:ArtifactItem; onChanged:() => void;
}) {
  const key = materialDraftKey(targetId,artifact);
  const draft = useSyncExternalStore(subscribe,() => readMaterialDraft(key),() => empty);
  const feedback = useRef<HTMLDivElement>(null);
  useEffect(() => { void loadMaterialDraft(key,artifact.path); },[key,artifact.path]);
  useEffect(() => {
    if (draft.error || draft.message) feedback.current?.scrollIntoView({block:"nearest"});
  },[draft.error,draft.message]);
  const edit = (patch:Partial<Pick<Draft,"text"|"note">>) => editMaterialDraft(key,patch);
  const save = async () => {
    if (await saveMaterialDraft(targetId,artifact)) onChanged();
  };
  return <>
    <p className="field-hint">{t("切换页面会保留本次 App 会话的草稿；关闭或重载 App 前请保存。保存 CV 时会同时校验并重排 PDF。")}</p>
    <label className="field"><span>{t("材料正文")}</span><textarea className="manual-editor" value={draft.text}
      onChange={event => edit({text:event.target.value})} disabled={!draft.loaded || draft.loading || draft.saving} /></label>
    <label className="field"><span>{t("修改说明（会自动记录为写作偏好）")}</span><input value={draft.note}
      onChange={event => edit({note:event.target.value})} disabled={!draft.loaded || draft.loading || draft.saving} placeholder={t("概括本次修改偏好，便于后续材料保持一致")} /></label>
    <div ref={feedback} className={`manual-save-feedback${draft.error ? " error" : ""}`} role={draft.error ? "alert" : "status"}>
      {materialDraftFeedback(key) || (draft.saving ? t("正在校验并保存，请稍候…") : draft.loading ? t("正在载入…") : dirty(draft) ? t("有未保存的修改（草稿已在本次会话保留）") : t("当前内容与已保存版本一致"))}
      {draft.loaded && draft.path !== artifact.path && dirty(draft) && <p>{t("正式材料可能已有新版本。你的输入已保留；保存时将核对原版本，防止覆盖更新。")}</p>}
    </div>
    <button className="button primary wide" disabled={!draft.loaded || draft.saving || draft.loading} onClick={() => void save()}>{draft.saving ? t("正在保存…") : t("保存为新版本")}</button>
    <button className="button secondary" disabled={draft.saving || draft.loading} onClick={() => {
      if (dirty(draft) && !window.confirm(t("丢弃当前未保存的修改，重新读取已保存版本？"))) return;
      void loadMaterialDraft(key,artifact.path,true);
    }}>{draft.loaded ? t("重新载入已保存版本") : t("重试载入")}</button>
  </>;
}
