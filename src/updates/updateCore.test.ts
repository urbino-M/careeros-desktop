import { afterEach, describe, expect, it } from "vitest";
import { formatLastChecked, formatUpdateMessage, updateErrorKey, updateErrorMessage, updateProgress } from "./updateCore";
import { setUiPreferences } from "../uiPreferences";

afterEach(() => setUiPreferences({ locale: "zh" }));

describe("desktop update helpers", () => {
  it("calculates bounded download progress", () => {
    expect(updateProgress(50, 200)).toBe(25);
    expect(updateProgress(250, 200)).toBe(100);
    expect(updateProgress(50)).toBeUndefined();
  });

  it("does not expose raw updater errors", () => {
    expect(updateErrorMessage(new Error("signature mismatch at https://example.invalid/token")))
      .toBe("更新包签名校验失败，已停止安装。请等待发布方修复。");
    expect(updateErrorMessage(new Error("opaque internal failure")))
      .not.toContain("opaque internal failure");
  });

  it("handles missing and invalid check timestamps", () => {
    expect(formatLastChecked()).toBe("尚未检查");
    expect(formatLastChecked("not-a-date")).toBe("尚未检查");
  });

  it("reformats retained update messages when the interface language changes", () => {
    const available = { key: "发现新版本 v{0}", values: ["0.2.1-preview"] };
    const error = updateErrorKey(new Error("signature mismatch at https://example.invalid/token"));
    expect(formatUpdateMessage(available)).toBe("发现新版本 v0.2.1-preview");
    setUiPreferences({ locale: "en" });
    expect(formatUpdateMessage(available)).toBe("Version 0.2.1-preview is available");
    expect(formatUpdateMessage(error)).toContain("signature could not be verified");
    expect(formatUpdateMessage(error)).not.toContain("example.invalid");
    expect(formatLastChecked()).toBe("Not checked yet");
    const timestamp = "2026-09-08T10:00:00Z";
    expect(formatLastChecked(timestamp)).toBe(new Date(timestamp).toLocaleString("en-US"));
    setUiPreferences({ locale: "zh" });
    expect(formatUpdateMessage(available)).toBe("发现新版本 v0.2.1-preview");
    expect(formatUpdateMessage(error)).toBe("更新包签名校验失败，已停止安装。请等待发布方修复。");
  });
});
