import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import { ApplicationsPage, DiscoveredOpportunityCard, opportunitySourceUrl, deadlineLabel, buildOpportunityContinuationRequest, OpportunityContinuationComposer } from "./ApplicationsPage";
import type { DiscoveredOpportunity, JobSummary } from "../types";
import { setUiPreferences } from "../uiPreferences";

afterEach(() => setUiPreferences({ locale: "zh" }));

const opportunity: DiscoveredOpportunity = {
  id: "opportunity-1", organization: "Example University", title: "Acoustics Postdoc",
  summary: "Research opportunity", country: null, region: null, deadline: null,
  sourceUrl: "https://example.edu/jobs", status: "discovered", discoveredAt: null, contacts: [],
};

describe("discovered opportunities", () => {
  it("shows unverified public leads with evidence and a scoped verification action", () => {
    setUiPreferences({locale:"en"});
    const lead:DiscoveredOpportunity={...opportunity,status:"open",verificationStatus:"unverified",sources:[{
      title:"原始证据",url:"https://example.invalid/post",checkedAt:"2026-09-09T00:00:00Z",channel:"linkedin",backend:"codex_web_search",evidenceType:"secondary",
    }]};
    const html=renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={lead} onNavigate={()=>{}}/>);
    expect(html).toContain("Source verification needed");
    expect(html).toContain("Verify this lead");
    expect(html).toContain("原始证据");
    expect(html).not.toContain("Materials complete");
    const request=buildOpportunityContinuationRequest(lead,"Verify the official source");
    expect(request.payload?.opportunityId).toBe(lead.id);
    expect(request.jobType).toBe("full_search");
  });
  it("switches card and list copy both ways without translating opportunity facts or task input", () => {
    const userOpportunity = { ...opportunity, organization: "用户大学", title: "用户保留的职位名称", summary: "用户保存的原始证据" };
    const before = buildOpportunityContinuationRequest(userOpportunity, "保留我的中文要求");
    setUiPreferences({ locale: "en" });
    const card = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={userOpportunity} onNavigate={() => {}} />);
    expect(card).toContain("Shelve opportunity");
    expect(card).toContain("Complete this opportunity");
    expect(card).toContain("Deadline not confirmed");
    expect(card).not.toContain("搁置这条机会");
    expect(card).toContain("用户大学");
    expect(card).toContain("用户保留的职位名称");
    expect(card).toContain("用户保存的原始证据");
    const page = renderToStaticMarkup(<ApplicationsPage careerSystem="postdoc" status="all" onNavigate={() => {}} />);
    expect(page).toContain("All Postdoc opportunities");
    expect(page).toContain("Advertised positions");
    expect(page).toContain("Prospective outreach");
    expect(page).toContain("Search PIs, institutions, roles or research topics…");
    expect(deadlineLabel("2026-09-12", new Date(2026, 8, 5))).toBe("2026-09-12 · 7 days remaining");
    expect(buildOpportunityContinuationRequest(userOpportunity, "保留我的中文要求")).toEqual(before);
    setUiPreferences({ locale: "zh" });
    expect(renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={userOpportunity} onNavigate={() => {}} />)).toContain("搁置这条机会");
    expect(deadlineLabel(null)).toBe("截止时间待确认");
  });

  it("offers shelving without a contact and restoration without material completion", () => {
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={opportunity} onNavigate={() => {}} />);
    expect(html).toContain("搁置这条机会");
    const shelved = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, shelved:true }} onNavigate={() => {}} />);
    expect(shelved).toContain("恢复这条机会");
    expect(shelved).not.toContain("继续完善这条机会");
    const mixed = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, contacts:[
      {id:"a",name:"Shelved PI",materialStatus:"pending",shelved:true},
      {id:"b",name:"Ready PI",materialStatus:"ready",shelved:false},
    ] }} onNavigate={() => {}} />);
    expect(mixed).toContain("已搁置 · 查看记录");
    expect(mixed).not.toContain("继续完善这条机会");
    const shelf = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, contacts:[
      {id:"a",name:"Shelved PI",materialStatus:"pending",shelved:true},
      {id:"b",name:"Active PI",materialStatus:"pending",shelved:false},
    ] }} shelvedOnly onNavigate={() => {}} />);
    expect(shelf).toContain("Shelved PI");
    expect(shelf).not.toContain("Active PI");
    expect(shelf).not.toContain("继续完善这条机会");
  });
  it("restores a saved job on first render and prevents starting another active continuation", () => {
    const latestJob: JobSummary = { id:"saved-job",jobType:"full_search",status:"running",progress:20,
      message:"正在补齐材料",providerId:"openai",createdAt:"2026-09-05T08:00:00Z",events:[],resultTargetIds:[] };
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, latestJob }} onNavigate={() => {}} />);
    expect(html).toContain("正在补齐材料");
    expect(html).toContain("查看进度 / 调整后重试");
    expect(html).not.toContain("新建补齐任务");
    expect(html).not.toContain("继续完善这条机会");
    const failed = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, latestJob:{ ...latestJob,status:"failed",error:"核验失败" } }} onNavigate={() => {}} />);
    expect(failed).toContain("核验失败");
    expect(failed).toContain("新建补齐任务");
    const request=buildOpportunityContinuationRequest({ ...opportunity,latestJob },"",undefined,2,"https://example.edu/verified-role");
    expect(request.payload?.confirmedSourceUrl).toBe("https://example.edu/verified-role");
    expect(request.payload?.opportunity).not.toHaveProperty("latestJob");
  });
  it("offers continuation for orphan and pending opportunities, not completed packages", () => {
    for (const contacts of [[], [{ id: "a", name: "Alpha", materialStatus: "pending" as const }]]) {
      const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, contacts }} onNavigate={() => {}} />);
      expect(html).toContain("继续完善这条机会");
    }
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, contacts: [{ id: "a", name: "Alpha", materialStatus: "ready" }] }} onNavigate={() => {}} />);
    expect(html).not.toContain("继续完善这条机会");
  });
  it("binds a continuation to the saved opportunity without passing its ID as a contact or resuming an unrelated thread", () => {
    const model = { providerId: "relay", modelId: "example", reasoning: "high" };
    const request = buildOpportunityContinuationRequest(opportunity, "  核验联系人  ", model);
    expect(request).toMatchObject({ jobType: "full_search", ...model, payload: { opportunityId: opportunity.id, instruction: "核验联系人", opportunity, threshold: 0, maxResults: 5 } });
    expect(request.targetId).toBeUndefined();
    expect(request.threadId).toBeUndefined();
    expect(buildOpportunityContinuationRequest(opportunity, "", model, 2).payload?.maxResults).toBe(2);
    expect(buildOpportunityContinuationRequest(opportunity, "", model, 20).payload?.maxResults).toBe(5);
    expect(request.prompt).toContain("Do not run a broad search");
    expect(request.prompt).toContain("preserve completed materials and manual edits");
    expect(buildOpportunityContinuationRequest({ ...opportunity, sourceUrl: "file:///private" }, "").payload?.opportunity).toMatchObject({ sourceUrl: null });
  });
  it("explains continuation scope and lets the user add instructions before enqueueing", () => {
    const html = renderToStaticMarkup(<OpportunityContinuationComposer opportunity={opportunity} onClose={() => {}} onCreated={() => {}} />);
    expect(html).toContain('role="dialog"');
    expect(html).toContain("补充要求（可选）");
    expect(html).toContain("不是恢复此前检索会话");
    expect(html).toContain("开始继续完善");
    expect(html).toContain("我已核对该官方页面确实对应当前机会");
  });
  it("offers type selection in every Postdoc status, without changing Internship", () => {
    for (const status of ["ready_to_contact", "contacted", "replied", "follow_up", "shelved", "all"] as const) {
      const html = renderToStaticMarkup(<ApplicationsPage careerSystem="postdoc" status={status} initialCategory="prospective" onNavigate={() => {}} />);
      expect(html).toContain("Postdoc 机会类型");
      expect(html).toContain("公开招聘岗位");
      expect(html).toContain('aria-selected="true" class="selected">套磁机会');
      expect(html).toContain("类型待核实");
    }
    const html = renderToStaticMarkup(<ApplicationsPage careerSystem="internship" status="all" onNavigate={() => {}} />);
    expect(html).not.toContain("Postdoc 机会类型");
  });
  it("does not label legacy open prospects as advertised positions", () => {
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, status: "open" }} category="prospective" onNavigate={() => {}} />);
    expect(html).toContain("潜在联系 · 非公开岗位");
    expect(html).not.toContain("公开招聘");
  });
  it("calculates calendar deadline hints without inventing dates for text deadlines", () => {
    const today = new Date(2026, 8, 5, 18);
    expect(deadlineLabel("2026-09-05", today)).toContain("今天截止");
    expect(deadlineLabel("2026-09-12", today)).toContain("剩余 7 天");
    expect(deadlineLabel("2026-09-04", today)).toContain("已截止");
    expect(deadlineLabel(null, today)).toBe("截止时间待确认");
    expect(deadlineLabel("until filled", today)).toBe("截止：until filled");
    expect(deadlineLabel("2026-02-30", today)).toContain("截止待核实");
  });
  it("adds a separate Postdoc entry without changing the internship tabs", () => {
    const postdoc = renderToStaticMarkup(<ApplicationsPage careerSystem="postdoc" status="ready_to_contact" onNavigate={() => {}} />);
    expect(postdoc.indexOf("已发现机会")).toBeLessThan(postdoc.indexOf("待处理"));
    const internship = renderToStaticMarkup(<ApplicationsPage careerSystem="internship" status="all" onNavigate={() => {}} />);
    expect(internship).not.toContain("已发现机会");
    const all = renderToStaticMarkup(<ApplicationsPage careerSystem="postdoc" status="all" onNavigate={() => {}} />);
    expect(all).toContain("全部 Postdoc 机会");
    expect(all).toContain("不受材料是否齐全或联系进度限制");
    expect(all).not.toContain("选择 Postdoc 申请");
  });
  it("shows an opportunity without a contact or materials and does not claim a legacy listing is open", () => {
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={opportunity} onNavigate={() => {}} />);
    expect(html).toContain("Acoustics Postdoc");
    expect(html).toContain("尚无联系人");
    expect(html).toContain("打开来源网页");
    expect(html).toContain("招聘状态待核实");
    expect(html).not.toContain("查看材料与联系记录");
    expect(html).toContain("材料待完成");
  });
  it("shows each associated contact and its material state", () => {
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, contacts: [
      { id: "a", name: "Alpha", materialStatus: "pending" }, { id: "b", name: "Beta", materialStatus: "ready" },
    ] }} onNavigate={() => {}} />);
    expect(html).toContain("Alpha · 材料待完成");
    expect(html).toContain("Beta · 查看材料与联系记录");
    expect(html).not.toContain("尚无联系人");
  });
  it("does not expose unsafe or missing source links", () => {
    for (const url of [null, "", "javascript:alert(1)", "file:///tmp/private", "https://user:secret@example.com"]) {
      expect(opportunitySourceUrl(url)).toBeUndefined();
    }
    expect(opportunitySourceUrl("https://example.edu/jobs")).toBe("https://example.edu/jobs");
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, sourceUrl: null }} onNavigate={() => {}} />);
    expect(html).toContain("来源链接待补充");
    expect(html).not.toContain("打开来源网页");
  });
  it("labels completed packages correctly in All instead of calling them discovered", () => {
    const html = renderToStaticMarkup(<DiscoveredOpportunityCard opportunity={{ ...opportunity, contacts: [
      { id: "a", name: "Alpha", materialStatus: "ready" }, { id: "b", name: "Beta", materialStatus: "ready" },
    ] }} onNavigate={() => {}} />);
    expect(html).toContain("材料已齐");
    expect(html).not.toContain("材料待完成");
    expect(html).not.toContain("已发现机会");
  });
});
