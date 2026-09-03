import { describe, expect, it } from "vitest";
import type { JobSummary } from "../types";
import { buildInternshipSearchQuery, buildTaskHistory, jobStatusMessage } from "./AutomationPage";

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
