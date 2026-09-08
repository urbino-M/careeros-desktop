import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";
import type { OnboardingProfile } from "../types";
import { OnboardingPage } from "./OnboardingPage";
import { setUiPreferences } from "../uiPreferences";
import { resolveUiNotice, uiNotice } from "../components/Ui";

const profile: OnboardingProfile = {
  schemaVersion: 1, completed: false, currentStep: 0,
  fullName: "", publicationName: "", careerStage: "not_specified", discipline: "not_specified",
  currentSituation: "", targetRoles: "", targetRegions: "", goals: "", constraints: "",
  preferredLanguage: "bilingual",
};
beforeEach(()=>setUiPreferences({locale:"zh"}));

describe("CV-first onboarding", () => {
  it("puts upload first and leaves background fields optional", () => {
    const html = renderToStaticMarkup(<OnboardingPage initial={profile} onComplete={() => {}} />);
    expect(html.indexOf("选择 CV 文件")).toBeLessThan(html.indexOf("补充或更正个人偏好"));
    expect(html).toContain("先进入，稍后上传");
    expect(html.slice(html.indexOf("<footer>"))).not.toContain("disabled=\"\"");
    expect(html).not.toContain("05</strong>");
    expect(html).toContain("交给你选择的模型服务处理");
  });

  it("accepts a CV with no mandatory stage, discipline or referee questionnaire", () => {
    const html = renderToStaticMarkup(<OnboardingPage initial={{ ...profile, cvSourceFile: "uploads/source.pdf" }} onComplete={() => {}} />);
    expect(html).toContain("保存并开始使用");
    expect(html).toContain("CV 已导入");
    expect(html).not.toContain("3 位推荐人");
  });

  it("switches interface copy while keeping profile values and material language independent", () => {
    const initial={...profile,fullName:"张研究者",goals:"保留我输入的研究目标",constraints:"原始约束",preferredLanguage:"zh" as const,careerStage:"doctoral",discipline:"social_sciences"};
    const render=()=>renderToStaticMarkup(<OnboardingPage initial={initial} onComplete={()=>{}} />);
    expect(render()).toContain("上传简历，就从这里开始");
    try {
      setUiPreferences({locale:"en"});
      const html=render();
      expect(html).toContain("Your next step starts with your CV");
      expect(html).toContain("Doctoral student / candidate");
      expect(html).toContain("Social sciences");
      expect(html).toContain("Material language");
      expect(html).toContain('value="zh" selected=""');
      expect(html).toContain("Interface language");
      expect(html).toContain(initial.fullName);
      expect(html).toContain(initial.goals);
      expect(html).toContain(initial.constraints);
    } finally { setUiPreferences({locale:"zh"}); }
    expect(render()).toContain("上传简历，就从这里开始");
    expect(initial.preferredLanguage).toBe("zh");
  });

  it("updates a saved connection notice in either UI language without translating its provider",()=>{
    const notice=uiNotice("{0} 已连接并完成 Responses 兼容性验证。","我的服务");
    try {
      setUiPreferences({locale:"en"});
      expect(resolveUiNotice(notice)).toBe("我的服务 connected and Responses compatibility verified.");
    } finally {setUiPreferences({locale:"zh"});}
    expect(resolveUiNotice(notice)).toBe("我的服务 已连接并完成 Responses 兼容性验证。");
  });
});
