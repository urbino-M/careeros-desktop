import { afterEach, describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { Activity } from "lucide-react";
import type { JobSummary } from "../types";
import { buildRetryRequest, buildTaskHistory, jobCardTitle, jobPhase, jobStatusMessage, JobSection, JobCard, buildInternshipSearchQuery } from "./AutomationPage";
import { setUiPreferences } from "../uiPreferences";

afterEach(() => setUiPreferences({ locale: "zh" }));

describe("Automation interface language", () => {
  it("translates task labels and durations while retaining the saved request and Agent instruction", () => {
    setUiPreferences({ locale: "en" });
    const item = { ...job("language-test", "completed", "2026-09-08T10:00:00Z"),
      jobType: "material_revision", requestSummary: "保留陈教授提供的 References", prompt: "原始 Agent 指令：请保留用户原文。",
      startedAt: "2026-09-08T10:00:00Z", finishedAt: "2026-09-08T11:05:00Z" };
    expect(jobCardTitle(item)).toBe("Revise: 保留陈教授提供的 References");
    const html = renderToStaticMarkup(createElement(JobCard, { job: item, onReload: () => {}, onNavigate: () => {} }));
    expect(html).toContain("Current stage");
    expect(html).toContain("1 hr 5 min");
    expect(html).toContain("Your request");
    expect(html).toContain(item.requestSummary);
    expect(html).toContain(item.prompt);
    setUiPreferences({ locale: "zh" });
    expect(jobCardTitle(item)).toBe("修订：保留陈教授提供的 References");
  });

  it("translates system guidance without rewriting a dynamic backend error", () => {
    setUiPreferences({ locale: "en" });
    expect(jobPhase({ status: "queued", progress: 0, events: [] }).label).toBe("Waiting to start");
    const message = "陈教授材料读取失败：/fixture/用户原文件.md";
    expect(jobStatusMessage(job("error-test", "failed", "2026-09-08T10:00:00Z", message))).toBe(message);
    expect(jobStatusMessage(job("error-test", "failed", "2026-09-08T10:00:00Z"))).toContain("The task failed.");
  });
});

describe("collapsible job groups", () => {
  it("keeps the count and hint visible and connects the toggle to retained content", () => {
    const html = renderToStaticMarkup(createElement(JobSection, {sectionId:"history",title:"任务记录",count:74,hint:"待处理 50",icon:Activity,children:"retained task content"}));
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain('aria-controls=');
    expect(html).toContain('hidden=""');
    expect(html).toContain("retained task content");
    expect(html).toContain("任务记录");
    expect(html).toContain("待处理 50");
    expect(html).toContain(">74</span>");
    expect(html).toContain("展开");
  });
  it("can initially expand running tasks without forcing other groups open", () => {
    const html = renderToStaticMarkup(createElement(JobSection,{sectionId:"running",title:"当前运行",count:1,hint:"执行中",icon:Activity,defaultOpen:true,children:"running task"}));
    expect(html).toContain('aria-expanded="true"');
    expect(html).not.toContain('hidden=""');
    expect(html).toContain("收起");
  });
});

function job(id: string, status: string, createdAt: string, error?: string): JobSummary {
  return {
    id,
    jobType: "full_search",
    resultTargetIds: [],
    status,
    progress: status === "failed" ? 100 : 50,
    providerId: "deepseek",
    modelId: "deepseek:deepseek-v4-pro",
    reasoning: "high",
    error,
    createdAt,
    events: [],
  };
}

describe("automation task history", () => {
  it("keeps newly failed jobs visible ahead of older review jobs", () => {
    const needsReview = Array.from({ length: 5 }, (_, index) =>
      job(`review-${index}`, "needs_review", `2026-09-01T10:0${index}:00Z`),
    );
    const failed = job("failed-new", "failed", "2026-09-01T19:31:34Z");

    expect(buildTaskHistory({ needsReview, recent: [failed, ...needsReview] }, 5).map((item) => item.id))
      .toContain("failed-new");
    expect(buildTaskHistory({ needsReview, recent: [failed, ...needsReview] }, 5)[0].id)
      .toBe("failed-new");
  });

  it("explains how to continue when a search turn ends without its result file", () => {
    const failed = job(
      "failed-search",
      "failed",
      "2026-09-01T19:31:34Z",
      "Agent 没有生成 /workspace/output/search-results.json: No such file or directory",
    );

    expect(jobStatusMessage(failed)).toContain("重新运行");
    expect(jobStatusMessage(failed)).toContain("恢复原线程");
  });

  it("labels model activity as execution instead of pretending a percentage is complete", () => {
    const running = job("running-search", "running", "2026-09-01T19:31:34Z");
    running.progress = 8;
    running.events = [{
      eventType: "activity",
      message: "正在压缩过长的模型上下文",
      createdAt: "2026-09-01T19:35:00Z",
    }];

    expect(jobPhase(running)).toEqual({
      label: "Agent 正在执行",
      detail: "模型正在检索、推理或整理任务结果。",
    });
  });

  it("uses the saved request as the task title instead of a generic full-search label", () => {
    const search = job("search-title", "failed", "2026-09-01T19:31:34Z");
    search.requestSummary = "  Trondheim 的海洋工程 Postdoc，优先流体力学方向  ";

    expect(jobCardTitle(search)).toBe("检索：Trondheim 的海洋工程 Postdoc，优先流体力学方向");
  });

  it("sends only adjusted retry fields so an unchanged retry can resume its thread", () => {
    const failed = job("retry-options", "failed", "2026-09-01T19:31:34Z");
    failed.prompt = "Research the original contact.";

    expect(buildRetryRequest(failed, "Research the original contact.", {
      providerId: "deepseek",
      modelId: "deepseek:deepseek-v4-pro",
      reasoning: "high",
    })).toEqual({ jobId: "retry-options" });

    expect(buildRetryRequest(failed, "Research the original contact.", {
      providerId: "deepseek",
      modelId: "deepseek:deepseek-v4-pro",
      reasoning: "high",
    }, 3)).toEqual({
      jobId: "retry-options",
      maxResults: 3,
    });

    expect(buildRetryRequest(failed, "Research a different contact direction.", {
      providerId: "minimax",
      modelId: "minimax:MiniMax-M2",
      reasoning: "medium",
    })).toEqual({
      jobId: "retry-options",
      prompt: "Research a different contact direction.",
      providerId: "minimax",
      modelId: "minimax:MiniMax-M2",
      reasoning: "medium",
    });
  });
});

describe("internship task composer", () => {
  it("builds an editable search request from the independent Internship profile", () => {
    expect(buildInternshipSearchQuery({
      schemaVersion: 1,
      targetRoles: "ML Engineer Intern",
      industries: "AI",
      regions: "Singapore",
      workMode: "hybrid",
      startDate: "2027 summer",
      duration: "12 weeks",
      workAuthorization: "待确认",
      enrollmentStatus: "硕士在读",
      constraints: "no relocation",
      rssFeeds: [],
    })).toContain("目标岗位 / 技能：ML Engineer Intern");
    expect(buildInternshipSearchQuery({
      schemaVersion: 1,
      targetRoles: "",
      industries: "",
      regions: "",
      workMode: "",
      startDate: "",
      duration: "",
      workAuthorization: "",
      enrollmentStatus: "",
      constraints: "",
      rssFeeds: [],
    })).toBe("寻找符合当前 Internship 画像的行业实习机会。");
  });
});
