import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

const postdocTarget = {
  id: "target-postdoc-cambridge",
  applicationId: "application-postdoc-cambridge",
  name: "Dr. Elena Rossi",
  email: "e.rossi@example.ac.uk",
  organization: "University of Cambridge",
  title: "Postdoctoral Research Associate",
  country: "United Kingdom",
  region: "Europe",
  fitScore: 94,
  priority: 1,
  status: "ready_to_contact",
  submissionStatus: "not_set",
  deadline: "2027-02-15",
  sourceUrl: "https://example.com/postdoc",
  updatedAt: "2026-09-02T08:00:00Z",
  careerTrack: "postdoc",
};

const internshipTarget = {
  id: "target-intern-deepmind",
  applicationId: "application-intern-deepmind",
  name: "官方申请门户",
  organization: "Google DeepMind",
  title: "Research Scientist Intern · 2027 Summer",
  country: "United Kingdom",
  region: "Europe",
  fitScore: 92,
  priority: 1,
  status: "ready_to_contact",
  submissionStatus: "portal_pending",
  deadline: "2026-12-01",
  sourceUrl: "https://example.com/internship",
  updatedAt: "2026-09-02T08:00:00Z",
  careerTrack: "internship",
};

const dashboards = {
  postdoc: {
    metrics: [
      { key: "all", label: "联系目标", value: 12, helper: "按 PI / 邮箱独立管理" },
      { key: "high_fit", label: "高匹配", value: 5, helper: "评分 ≥ 85" },
      { key: "ready_to_contact", label: "待联系", value: 4, helper: "尚未确认发送" },
    ],
    regions: [
      { region: "Europe", count: 5 },
      { region: "Asia", count: 4 },
      { region: "North America", count: 3 },
    ],
    priorityTargets: [postdocTarget],
  },
  internship: {
    metrics: [
      { key: "all", label: "申请机会", value: 8, helper: "只显示行业 Internship" },
      { key: "high_fit", label: "高匹配", value: 3, helper: "评分 ≥ 85" },
      { key: "portal_pending", label: "待投递", value: 4, helper: "已核验，等待官网投递" },
    ],
    regions: [
      { region: "North America", count: 4 },
      { region: "Europe", count: 2 },
      { region: "Asia", count: 2 },
    ],
    priorityTargets: [internshipTarget],
  },
};

const detail = (target: typeof postdocTarget | typeof internshipTarget) => ({
  target,
  summary: target.careerTrack === "internship"
    ? "研究型实习机会，工作内容覆盖多模态学习、评测与可靠性。"
    : "面向海洋机器人与水下声学方向的博士后研究机会。",
  department: target.careerTrack === "internship" ? "Research · Machine Intelligence" : "Department of Engineering",
  piResearchSummary: "公开页面显示团队持续开展机器学习与自主系统研究。",
  applicationNotes: "优先确认资格、截止日期与官方申请入口；任何外部操作均需手动确认。",
  artifacts: [],
  checklist: target.careerTrack === "internship"
    ? [
        { id: "degree", itemType: "degree", required: true, status: "verified", evidence: "在读博士或相关研究经历（待按职位页最终确认）" },
        { id: "resume", itemType: "resume", required: true, status: "pending", note: "准备一页英文简历" },
        { id: "portal", itemType: "application_portal", required: true, status: "pending", note: "通过官方职位页提交" },
      ]
    : [],
  replies: [],
  revisions: [],
});

const jobs = {
  running: [
    {
      id: "job-preview-internship",
      jobType: "internship_search",
      resultTargetIds: [],
      status: "running",
      progress: 68,
      message: "正在核验官方职位页与硬性资格条件…",
      providerId: "openai",
      modelId: "gpt-5",
      reasoning: "high",
      createdAt: "2026-09-02T08:12:00Z",
    },
  ],
  queued: [
    {
      id: "job-preview-followup",
      jobType: "follow_up_scan",
      resultTargetIds: [],
      status: "queued",
      progress: 0,
      message: "等待 Worker 领取",
      providerId: "openai",
      createdAt: "2026-09-02T08:16:00Z",
    },
  ],
  needsReview: [],
  needsReviewTotal: 0,
  recent: [],
  recentTotal: 0,
  capacity: 5,
};

const provider = {
  id: "openai",
  displayName: "OpenAI / Codex",
  connectionMode: "internal_gateway",
  enabled: true,
  models: [
    { id: "gpt-5", slug: "gpt-5", displayName: "GPT-5", enabled: true, supportsReasoning: true, supportsTools: true },
  ],
};

const defaults = [
  { taskType: "full_search", providerId: "openai", modelId: "gpt-5", reasoning: "high" },
  { taskType: "research_pi", providerId: "openai", modelId: "gpt-5", reasoning: "high" },
  { taskType: "maintenance", providerId: "openai", modelId: "gpt-5", reasoning: "medium" },
];

function payloadValue(payload: unknown, key: string) {
  if (!payload || typeof payload !== "object") return undefined;
  return (payload as Record<string, unknown>)[key];
}

mockWindows("main");
mockIPC((command, payload) => {
  if (command === "get_dashboard") {
    return dashboards[payloadValue(payload, "careerTrack") === "internship" ? "internship" : "postdoc"];
  }
  if (command === "get_contact_targets") {
    return payloadValue(payload, "careerTrack") === "internship" ? [internshipTarget] : [postdocTarget];
  }
  if (command === "get_contact_target") {
    const targetId = payloadValue(payload, "targetId");
    return detail(targetId === internshipTarget.id ? internshipTarget : postdocTarget);
  }
  if (command === "get_jobs") return jobs;
  if (command === "get_model_providers") return [provider];
  if (command === "get_task_model_defaults") return defaults;
  if (command === "get_migration_report") {
    return { imported: true, applications: 20, opportunities: 20, legacyJobs: 4, revisions: 7, gmailDrafts: 0, activeTargets: 20, hiddenTombstones: 0 };
  }
  if (command === "get_gmail_status") {
    return { configured: false, connected: false, expectedEmail: "urbinohbmiao@gmail.com", connectionOk: false, oauthStatus: "idle" };
  }
  if (command === "get_codex_account") return {};
  if (command === "has_openai_api_key") return false;
  if (command === "get_cv_approval") return false;
  if (command === "list_gmail_drafts") return [];
  return null;
}, { shouldMockEvents: true });

await import("./main");
