import { createElement, useState } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import { InternshipPlanningPanel } from "../components/InternshipPlanningPanel";
import { SettingsPage } from "./SettingsPage";

vi.mock("react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react")>();
  return { ...actual, useState: vi.fn(actual.useState) };
});
vi.mock("../updates/UpdateManager", () => ({
  useDesktopUpdates: () => ({
    phase: "idle", currentVersion: "test", message: "Ready",
    lastCheckedLabel: "Never", checkNow: vi.fn(),
  }),
}));

afterEach(() => vi.clearAllMocks());

describe("search channel extension removal", () => {
  it("keeps account, CV and update settings without channel setup or login controls", () => {
    // Render the loaded page; remaining hooks use React's real implementation.
    vi.mocked(useState)
      .mockImplementationOnce(() => [[], vi.fn()])
      .mockImplementationOnce(() => [[], vi.fn()])
      .mockImplementationOnce(() => [{
        schemaVersion: 1, enabled: false, emphasize: "", exclude: "", instructions: "",
      }, vi.fn()]);
    const html = renderToStaticMarkup(createElement(SettingsPage, { onRestartOnboarding: vi.fn() }));
    for (const label of ["OpenAI / Codex", "任务默认模型", "CV 定制", "Gmail 草稿", "应用更新"]) {
      expect(html).toContain(label);
    }
    for (const label of ["管理信息搜索渠道", "一键启用", "连接渠道", "search-channels"]) {
      expect(html).not.toContain(label);
    }
  });

  it("keeps Internship profile, CV upload and search launch without channel prerequisites", () => {
    const html = renderToStaticMarkup(createElement(InternshipPlanningPanel, { onNavigate: vi.fn() }));
    for (const label of ["独立的 Internship 画像", "选择 CV 文件", "Internship CV 上传区域", "保存 Internship 画像", "在 Agent 中启动全面扫描", "GPT / Codex", "LinkedIn / X"]) {
      expect(html).toContain(label);
    }
    for (const label of ["管理信息搜索渠道", "个渠道可用", "RSS / Atom", "正在检查渠道", "channel-summary"]) {
      expect(html).not.toContain(label);
    }
  });

  it("does not expose retired installation, health or social-login commands", () => {
    expect(api).not.toHaveProperty("searchCapabilities");
    expect(api).not.toHaveProperty("setupSearchCapabilities");
    expect(api).not.toHaveProperty("beginSearchChannelAuth");
    expect(api.internshipProfile).toBeTypeOf("function");
    expect(api.importInternshipCv).toBeTypeOf("function");
    expect(api.enqueue).toBeTypeOf("function");
  });
});
