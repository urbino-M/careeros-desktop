import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";
import type { CvCustomizationSettings, ProviderInfo, TaskModelDefault } from "../types";
import { setUiPreferences } from "../uiPreferences";
import { resolveUiNotice, uiNotice } from "../components/Ui";
import { t } from "../i18n";
import { CvCustomizationCard, DefaultRow, ProviderConnectionSettings, providerValidationLabel } from "./SettingsPage";

const provider: ProviderInfo = {
  id:"openai",displayName:"用户自定义服务名称",adapterKind:"codex",connectionMode:"oauth",enabled:true,configured:true,
  models:[{id:"openai:gpt-5.6-terra",slug:"gpt-5.6-terra",displayName:"Terra · 平衡",enabled:true,supportsReasoning:true,supportsTools:true,supportsVision:true,reasoningLevels:["low","high"]}],
};
const defaults: TaskModelDefault = {taskType:"full_search",providerId:provider.id,modelId:provider.models[0].id,reasoning:"high"};
const customization: CvCustomizationSettings = {schemaVersion:1,enabled:true,emphasize:"原始研究方向",exclude:"不要更改的自由文本",instructions:"Keep my exact wording.",pageCount:{mode:"fixed",value:3},preserveStructure:false};
beforeEach(() => setUiPreferences({locale:"zh"}));

describe("settings UI language", () => {
  it("switches default-model labels while preserving selection values and provider names", () => {
    const render = () => renderToStaticMarkup(<DefaultRow value={defaults} providers={[provider]} onSaved={()=>{}} />);
    expect(render()).toContain("Terra · 平衡");
    try {
      setUiPreferences({locale:"en"});
      const html=render();
      expect(html).toContain("Full search");
      expect(html).toContain("Terra · Balanced");
      expect(html).toContain('value="high" selected=""');
      expect(html).toContain("High");
      expect(html).toContain(provider.displayName);
      expect(html).toContain(`value="${defaults.modelId}"`);
    } finally { setUiPreferences({locale:"zh"}); }
  });

  it("does not translate provider-defined names that happen to match UI messages", () => {
    const remote={...provider,id:"remote",displayName:"设置",models:[{...provider.models[0],id:"remote:custom",displayName:"Sol · 最高质量"}]};
    try {
      setUiPreferences({locale:"en"});
      const html=renderToStaticMarkup(<ProviderConnectionSettings providers={[remote]} onChanged={()=>{}} />);
      expect(html).toContain("Verify &amp; connect");
      expect(html).toContain("设置");
      expect(html).toContain("Sol · 最高质量");
      expect(html).not.toContain("Sol · Highest quality");
    } finally { setUiPreferences({locale:"zh"}); }
  });

  it("localizes CV controls without changing user instructions or page count", () => {
    const render=()=>renderToStaticMarkup(<CvCustomizationCard value={customization} onSaved={()=>{}} onError={()=>{}} />);
    expect(render()).toContain("重点强调");
    try {
      setUiPreferences({locale:"en"});
      const html=render();
      expect(html).toContain("Emphasize");
      expect(html).toContain("Save CV preferences");
      expect(html).toContain(customization.emphasize);
      expect(html).toContain(customization.exclude);
      expect(html).toContain(customization.instructions);
      expect(html).toContain('value="3"');
    } finally { setUiPreferences({locale:"zh"}); }
  });

  it("retranslates an existing parameterized notice while leaving its provider name intact", () => {
    const notice=uiNotice("{0} 已连接；发现 {1} 个可用模型。","设置",2);
    const saved=uiNotice("{0}默认模型已保存。",()=>t("完整检索"));
    expect(resolveUiNotice(notice)).toBe("设置 已连接；发现 2 个可用模型。");
    try {
      setUiPreferences({locale:"en"});
      expect(resolveUiNotice(notice)).toBe("设置 connected; 2 model(s) available.");
      expect(resolveUiNotice(saved)).toBe("Default model saved for Full search.");
      expect(providerValidationLabel("Responses 已验证；发现 2 个可用模型（探测模型：用户模型）")).toBe("Responses verified; 2 model(s) available (tested model: 用户模型)");
      expect(providerValidationLabel("Provider-specific original detail")).toBe("Provider-specific original detail");
    } finally { setUiPreferences({locale:"zh"}); }
    expect(resolveUiNotice(notice)).toBe("设置 已连接；发现 2 个可用模型。");
  });
});
