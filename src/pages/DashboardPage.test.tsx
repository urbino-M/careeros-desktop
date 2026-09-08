import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import { DashboardContent, DashboardOpportunitySections, dashboardDeadlineHint } from "./DashboardPage";
import { parseHash, routeHash } from "../App";
import type { DashboardData, DiscoveredOpportunity, DiscoveredOpportunityPage, OpportunityCategory, TargetCard } from "../types";
import { InternshipPlanningPanel, InternshipPlanningSummary } from "../components/InternshipPlanningPanel";
import { setUiPreferences } from "../uiPreferences";

afterEach(() => setUiPreferences({ locale: "zh" }));

const empty: DiscoveredOpportunityPage = { items: [], total: 0, overallTotal: 0, pendingTotal: 0 };
const postdocOpportunity: DiscoveredOpportunity = { id: "p", title: "Research vacancy", organization: "Academic-only Lab", summary: null, country: "Norway", region: null, deadline: "2099-01-02", sourceUrl: null, status: "open", discoveredAt: null, contacts: [] };
const internTarget: TargetCard = { id: "i", applicationId: "a", name: "Portal", title: "Industry vacancy", organization: "Intern-only Company", priority: 0, status: "ready_to_contact", submissionStatus: "submitted", updatedAt: "2026-09-05", careerTrack: "internship", materialStatus: "ready", deadline: "2099-01-01" };
const metrics = (values: Record<string, number>) => Object.entries(values).map(([key, value]) => ({ key, value, label: key, helper: "fixture" }));
const postdoc: DashboardData = { metrics: metrics({ all: 2, high_fit: 1, ready_to_contact: 1, replied: 1, follow_up: 0 }), regions: [{ region: "AcademicRegion", count: 2 }], priorityTargets: [] };
const internship: DashboardData = { metrics: metrics({ all: 1, high_fit: 1, portal_pending: 0, submitted: 1, not_set: 0, not_required: 0 }), regions: [{ region: "IndustryRegion", count: 1 }], priorityTargets: [internTarget] };
const bothTracks = { postdoc, internship, opportunities: { advertised: { ...empty, items: [postdocOpportunity], total: 1, overallTotal: 3, pendingTotal: 1 }, prospective: { ...empty, items: [{ ...postdocOpportunity, id: "cold", organization: "Prospect-only Lab", status: "prospective" }], total: 1 }, uncertain: empty } };

describe("dashboard track isolation", () => {
  const render = (view: "overview" | "postdoc" | "internship", data = bothTracks) => renderToStaticMarkup(<DashboardContent view={view} data={data} onViewChange={() => {}} onNavigate={() => {}} />);
  it("localizes overview, metrics and deadline hints without altering saved organizations or regions", () => {
    setUiPreferences({ locale: "en" });
    const overview = render("overview", { ...bothTracks, postdoc: { ...postdoc, metrics: postdoc.metrics.map((metric) => ({ ...metric, helper: "评分 ≥ 85" })) } });
    expect(overview).toContain("Your applications, at a glance.");
    expect(overview).toContain("Overview");
    expect(overview).toContain("High-fit contacts");
    expect(overview).toContain("Score ≥ 85");
    expect(overview).toContain("Application records");
    expect(overview).not.toContain("申请进展，一眼看清。");
    const track = render("postdoc", { ...bothTracks, opportunities: { ...bothTracks.opportunities, advertised: { ...bothTracks.opportunities.advertised, items: [{ ...postdocOpportunity, organization: "用户保存的大学" }] } } });
    expect(track).toContain("Advertised positions");
    expect(track).toContain("Prospective outreach");
    expect(track).toContain("用户保存的大学");
    expect(track).toContain("AcademicRegion");
    expect(dashboardDeadlineHint([])).toBe("No known upcoming deadline");
    setUiPreferences({ locale: "zh" });
    expect(render("overview")).toContain("申请进展，一眼看清。");
    expect(dashboardDeadlineHint([])).toBe("暂无已知未截止日期");
  });
  it("localizes planning presets and mapped placeholder labels", () => {
    setUiPreferences({ locale: "en" });
    const panel = renderToStaticMarkup(<InternshipPlanningPanel onNavigate={() => {}} />);
    expect(panel).toContain("Track A");
    expect(panel).toContain("Role track A");
    expect(panel).toContain("Project 01");
    expect(panel).toContain("Verified contribution");
    expect(panel).toContain("Not configured");
    expect(panel).toContain("Start a full scan with the Agent");
    expect(panel).not.toContain("岗位方向 A");
    const summary = renderToStaticMarkup(<InternshipPlanningSummary onNavigate={() => {}} />);
    expect(summary).toContain("Work arrangement");
    expect(summary).toContain("View career strategy");
    expect(summary).not.toContain("工作方式");
  });
  it("overview contains only track summaries, not mixed listings or charts", () => {
    const html = render("overview");
    expect(html).toContain("两条申请轨道概览");
    expect(html).toContain("高匹配联系人（人）");
    expect(html).toContain("申请记录（条）");
    expect(html).toContain("最近已知截止");
    for (const text of ["Academic-only Lab", "Intern-only Company", "Prospect-only Lab", "AcademicRegion", "IndustryRegion", "Postdoc 需要处理"]) expect(html).not.toContain(text);
  });
  it("Postdoc contains only academic listings, alerts and regions", () => {
    const html = render("postdoc");
    for (const text of ["Academic-only Lab", "Prospect-only Lab", "AcademicRegion", "Postdoc 需要处理"]) expect(html).toContain(text);
    for (const text of ["Intern-only Company", "IndustryRegion", "Internship 投递进度", "两条申请轨道概览"]) expect(html).not.toContain(text);
  });
  it("Internship contains only industry listings and submission progress", () => {
    const html = render("internship");
    for (const text of ["Intern-only Company", "IndustryRegion", "Internship 投递进度", "实习岗位 · 截止优先"]) expect(html).toContain(text);
    for (const text of ["Academic-only Lab", "Prospect-only Lab", "AcademicRegion", "Postdoc 需要处理"]) expect(html).not.toContain(text);
  });
  it("an empty Internship track does not fall back to Postdoc opportunities", () => {
    const html = render("internship", { ...bothTracks, internship: { ...internship, regions: [], priorityTargets: [] } });
    expect(html).toContain("还没有实习岗位");
    expect(html).not.toContain("Academic-only Lab");
  });
  it("summarizes calendar deadlines without treating text as a date", () => {
    const today = new Date(2026, 8, 5);
    expect(dashboardDeadlineHint([null, "until filled", "2026-09-04", "2026-09-10", "2026-09-06"], today)).toContain("剩余 1 天");
    expect(dashboardDeadlineHint(["2026-02-30", "2026-09-04"], today)).toBe("暂无已知未截止日期");
  });
});

describe("dashboard opportunity sections", () => {
  it("keeps advertised and prospective sections visible when empty", () => {
    const html = renderToStaticMarkup(<DashboardOpportunitySections opportunities={{ advertised: empty, prospective: empty, uncertain: empty }} onNavigate={() => {}} />);
    expect(html).toContain("公开招聘岗位");
    expect(html).toContain("套磁机会");
    expect(html).toContain("按截止时间");
    expect(html).toContain("按匹配度");
    expect(html).not.toContain("查看待核实记录");
  });
  it("keeps uncertain records accessible rather than treating them as cold outreach", () => {
    const html = renderToStaticMarkup(<DashboardOpportunitySections opportunities={{ advertised: empty, prospective: empty, uncertain: { ...empty, total: 3 } }} onNavigate={() => {}} />);
    expect(html).toContain("3 条机会的类型待核实");
    expect(html).toContain("查看待核实记录");
  });
  it("round trips category links and preserves existing hashes", () => {
    for (const category of ["advertised", "prospective", "uncertain"] as OpportunityCategory[]) {
      const route = { page: "applications" as const, careerSystem: "postdoc" as const, status: "all" as const, category };
      expect(parseHash(routeHash(route))).toMatchObject(route);
    }
    expect(parseHash("#/applications/postdoc/contacted")).toMatchObject({ status: "contacted", category: undefined });
    expect(parseHash("#/applications/postdoc/all/invalid").page).toBe("applications");
    expect(parseHash("#/applications/internship/strategy")).toMatchObject({ careerSystem: "internship", view: "strategy", category: undefined });
  });
});
