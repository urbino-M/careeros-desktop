import { describe, expect, it } from "vitest";
import { formatLastChecked, updateErrorMessage, updateProgress } from "./updateCore";

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
});
