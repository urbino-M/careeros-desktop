import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import type { ArtifactItem, RevisionResult } from "../types";
import { editMaterialDraft, loadMaterialDraft, materialDraftKey, materialDraftFeedback, readMaterialDraft, saveMaterialDraft } from "./ManualMaterialEditor";
import { t } from "../i18n";
import { setUiPreferences } from "../uiPreferences";

vi.mock("../api", () => ({
  api: { readMaterial: vi.fn(), saveManualMaterial: vi.fn() },
  errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error),
}));

const artifact: ArtifactItem = { artifactType:"email",language:"en",path:"/fixture/original.md",exists:true,updatedAt:"2026-09-08" };
const original = "Dear Professor,\r\nI would like to discuss your research.\r\n";
const edited = "Dear Professor,\nPlease find my revised research enquiry.\n";
const originalHash = "53a5115b973ec7d9720be128b61e67f04ff429de8ac9270a5903302c6e8bc682";
const editedHash = "bdd83e82d98860e1d3cc6ab317baeb85ead23bad8b9ee9cd074241673e5fb58e";
const revision: RevisionResult = { revisionId:"revision",artifactPath:"/fixture/revised.md",backupPath:artifact.path,summary:"Updated letter",locations:[],diff:[] };

beforeEach(() => { setUiPreferences({locale:"zh"}); vi.mocked(api.readMaterial).mockReset(); vi.mocked(api.saveManualMaterial).mockReset(); });

describe("shared contact letter drafts", () => {
  it("keeps the unload warning owned by unsaved session drafts until they are discarded", async () => {
    const addEventListener = vi.fn(), removeEventListener = vi.fn();
    vi.stubGlobal("window", {addEventListener,removeEventListener});
    try {
      const key = materialDraftKey("closing-letter",artifact);
      vi.mocked(api.readMaterial).mockResolvedValue(original);
      await loadMaterialDraft(key,artifact.path);
      editMaterialDraft(key,{text:edited});
      expect(addEventListener).toHaveBeenCalledWith("beforeunload",expect.any(Function));
      // No editor is mounted here: the session draft alone keeps the warning active.
      const event = {preventDefault:vi.fn(),returnValue:undefined};
      addEventListener.mock.calls[0][1](event);
      expect(event.preventDefault).toHaveBeenCalledOnce();
      expect(removeEventListener).not.toHaveBeenCalled();
      await loadMaterialDraft(key,artifact.path,true);
      expect(removeEventListener).toHaveBeenCalledWith("beforeunload",addEventListener.mock.calls[0][1]);
    } finally { vi.unstubAllGlobals(); }
  });

  it("retains the original baseline and input when a newer published version rejects saving", async () => {
    const target = "conflicting-letter", key = materialDraftKey(target,artifact);
    vi.mocked(api.readMaterial).mockResolvedValue(original);
    await loadMaterialDraft(key,artifact.path);
    editMaterialDraft(key,{text:edited,note:"Keep the factual wording"});
    const newer = {...artifact,path:"/fixture/newer.md"};
    await loadMaterialDraft(key,newer.path);
    expect(api.readMaterial).toHaveBeenCalledTimes(1);
    vi.mocked(api.saveManualMaterial).mockRejectedValue(new Error("原版本已变化"));
    expect(await saveMaterialDraft(target,newer)).toBe(false);
    expect(api.saveManualMaterial).toHaveBeenCalledWith({ targetId:target,artifactType:"email",language:"en",
      content:edited,note:"Keep the factual wording",expectedBaseSha256:originalHash });
    expect(readMaterialDraft(key)).toMatchObject({text:edited,original,path:artifact.path,baseHash:originalHash,saving:false});
    expect(materialDraftFeedback(key)).toContain("输入已保留");
  });

  it("keeps a draft when reopening either editor and isolates targets and languages", async () => {
    const target = "returning-letter", key = materialDraftKey(target,artifact);
    vi.mocked(api.readMaterial).mockResolvedValue(original);
    await loadMaterialDraft(key,artifact.path);
    editMaterialDraft(key,{text:edited});
    // Both the direct letter page and revision page reopen the same target/type/language.
    await loadMaterialDraft(materialDraftKey(target,{...artifact}),artifact.path);
    const newer = {...artifact,path:"/fixture/newer.md"};
    await loadMaterialDraft(materialDraftKey(target,newer),newer.path);
    expect(readMaterialDraft(key).text).toBe(edited);
    expect(api.readMaterial).toHaveBeenCalledTimes(1);
    const zh = materialDraftKey(target,{...artifact,language:"zh"});
    const other = materialDraftKey("another-contact",artifact);
    await loadMaterialDraft(zh,"/fixture/zh.md");
    await loadMaterialDraft(other,artifact.path);
    expect(readMaterialDraft(zh).text).toBe(original);
    expect(readMaterialDraft(other).text).toBe(original);
    expect(readMaterialDraft(key).text).toBe(edited);
  });

  it("refreshes the baseline after success and blocks concurrent saves and edits", async () => {
    const target = "saving-letter", key = materialDraftKey(target,artifact);
    vi.mocked(api.readMaterial).mockResolvedValueOnce(original).mockResolvedValue(edited);
    await loadMaterialDraft(key,artifact.path);
    editMaterialDraft(key,{text:edited});
    let finish!: (value: RevisionResult) => void;
    vi.mocked(api.saveManualMaterial).mockImplementationOnce(() => new Promise(resolve => { finish=resolve; }));
    const saving = saveMaterialDraft(target,artifact);
    editMaterialDraft(key,{text:"Must not replace the saving snapshot"});
    expect(await saveMaterialDraft(target,artifact)).toBe(false);
    expect(api.saveManualMaterial).toHaveBeenCalledTimes(1);
    finish(revision);
    expect(await saving).toBe(true);
    expect(readMaterialDraft(key)).toMatchObject({text:edited,original:edited,path:revision.artifactPath,baseHash:editedHash,saving:false});
    editMaterialDraft(key,{text:edited+"Further clarification."});
    vi.mocked(api.saveManualMaterial).mockRejectedValue(new Error("fixture stop"));
    await saveMaterialDraft(target,{...artifact,path:revision.artifactPath});
    expect(api.saveManualMaterial).toHaveBeenLastCalledWith(expect.objectContaining({expectedBaseSha256:editedHash}));
  });

  it("reports a successful save truthfully when rereading fails and requires a fresh baseline", async () => {
    const target = "read-failed-letter", key = materialDraftKey(target,artifact);
    vi.mocked(api.readMaterial).mockResolvedValueOnce(original).mockRejectedValue(new Error("read failed"));
    await loadMaterialDraft(key,artifact.path);
    editMaterialDraft(key,{text:edited});
    vi.mocked(api.saveManualMaterial).mockResolvedValue(revision);
    expect(await saveMaterialDraft(target,artifact)).toBe(true);
    expect(readMaterialDraft(key)).toMatchObject({text:edited,original:edited,path:revision.artifactPath,loaded:false,saving:false,error:""});
    expect(materialDraftFeedback(key)).toContain("已保存新版本");
    expect(await saveMaterialDraft(target,artifact)).toBe(false);
    expect(api.saveManualMaterial).toHaveBeenCalledTimes(1);
    vi.mocked(api.readMaterial).mockResolvedValue(edited);
    await loadMaterialDraft(key,revision.artifactPath,true);
    expect(readMaterialDraft(key)).toMatchObject({loaded:true,baseHash:editedHash});
  });

  it("ignores a late response from an explicitly replaced load", async () => {
    const key = materialDraftKey("loading-letter",artifact);
    let finishOld!: (text: string) => void;
    vi.mocked(api.readMaterial).mockImplementationOnce(() => new Promise(resolve => {finishOld=resolve;})).mockResolvedValue(edited);
    const stale = loadMaterialDraft(key,artifact.path);
    await loadMaterialDraft(key,revision.artifactPath,true);
    finishOld(original);
    await stale;
    expect(readMaterialDraft(key)).toMatchObject({text:edited,path:revision.artifactPath,baseHash:editedHash});
  });

  it("changes UI language without translating or resetting an unsaved draft", async () => {
    const key = materialDraftKey("bilingual-letter",artifact);
    vi.mocked(api.readMaterial).mockResolvedValue(original);
    await loadMaterialDraft(key,artifact.path);
    editMaterialDraft(key,{text:"尊敬的教授，my original bilingual text.",note:"保留这个表达"});
    vi.mocked(api.saveManualMaterial).mockRejectedValue(new Error("fixture conflict"));
    await saveMaterialDraft("bilingual-letter",artifact);
    const snapshot = readMaterialDraft(key);
    try {
      setUiPreferences({locale:"en"});
      expect(t("保存为新版本")).toBe("Save as a new version");
      expect(materialDraftFeedback(key)).toContain("Your input is kept");
      await loadMaterialDraft(key,artifact.path);
      expect(readMaterialDraft(key)).toBe(snapshot);
      expect(api.readMaterial).toHaveBeenCalledTimes(1);
    } finally { setUiPreferences({locale:"zh"}); }
  });
});
