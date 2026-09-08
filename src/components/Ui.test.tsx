import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";
import { setUiPreferences } from "../uiPreferences";
import { LoadingState, StatusBadge, SubmissionBadge, formatLocalTime, resolveUiNotice, uiNotice } from "./Ui";
import { ModelControls, modelDisplayLabel, reasoningLabel } from "./ModelControls";

beforeEach(()=>setUiPreferences({locale:"zh"}));
describe("shared UI language",()=>{
  it("updates state labels, model controls and dates without changing machine values",()=>{
    const zh=formatLocalTime("2026-09-08T05:06:00Z");
    expect(renderToStaticMarkup(<StatusBadge status="replied" />)).toContain("已回复");
    try {
      setUiPreferences({locale:"en"});
      expect(renderToStaticMarkup(<StatusBadge status="replied" />)).toContain("Replied");
      expect(renderToStaticMarkup(<StatusBadge status="running" />)).toContain("Running");
      expect(renderToStaticMarkup(<SubmissionBadge status="submitted" />)).toContain("Submitted");
      expect(renderToStaticMarkup(<LoadingState />)).toContain("Loading local data");
      const html=renderToStaticMarkup(<ModelControls taskType="full_search" value={{providerId:"openai",modelId:"model",reasoning:"high"}} onChange={()=>{}} />);
      expect(html).toContain("Reasoning effort");
      expect(html).toContain('value="high" selected=""');
      expect(html).toContain("High");
      expect(formatLocalTime("2026-09-08T05:06:00Z")).not.toBe(zh);
      expect(modelDisplayLabel("openai","Luna · 快速")).toBe("Luna · Fast");
      expect(modelDisplayLabel("custom","Luna · 快速")).toBe("Luna · 快速");
      expect(modelDisplayLabel("openai","Unknown model name")).toBe("Unknown model name");
      expect(reasoningLabel("custom-effort")).toBe("custom-effort");
    } finally { setUiPreferences({locale:"zh"}); }
  });
  it("keeps notice parameters literal when they contain translation syntax",()=>{
    const notice=uiNotice("{0} 已连接并完成 Responses 兼容性验证。","用户 {1} 模型");
    try {
      setUiPreferences({locale:"en"});
      expect(resolveUiNotice(notice)).toBe("用户 {1} 模型 connected and Responses compatibility verified.");
    } finally { setUiPreferences({locale:"zh"}); }
  });
});
