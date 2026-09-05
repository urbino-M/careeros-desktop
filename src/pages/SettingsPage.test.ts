import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import type { AuthGuide, ChannelHealth, SearchCapabilities } from "../types";
import { channelState, connectSearchChannel, pollSearchChannelAuth } from "./SettingsPage";

vi.mock("../api", () => ({
  api: { searchCapabilities: vi.fn(), beginSearchChannelAuth: vi.fn() },
  errorMessage: String,
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

function capabilities(authenticated: boolean, status = authenticated ? "ready" : "login_required"): SearchCapabilities {
  return {
    checkedAt: "2026-09-05T10:00:00Z", warnings: [],
    channels: [{ channel: "facebook", backend: "opencli:facebook", available: true,
      authenticated, status, message: "safe status", checkedAt: "2026-09-05T10:00:00Z" }],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.useFakeTimers();
  vi.stubGlobal("window", globalThis);
});
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("search channel connection", () => {
  it("waits for the backend login to finish before reading its session", async () => {
    const login = deferred<AuthGuide>();
    vi.mocked(api.beginSearchChannelAuth).mockReturnValue(login.promise);
    vi.mocked(api.searchCapabilities).mockResolvedValue(capabilities(true));
    const onGuide = vi.fn();
    const pending = connectSearchChannel("facebook", onGuide);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(api.searchCapabilities).not.toHaveBeenCalled();
    login.resolve({ channel: "facebook", title: "Connect", url: null, instructions: [] });
    expect(await pending).toEqual(capabilities(true));
    expect(api.searchCapabilities).toHaveBeenCalledTimes(1);
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("opening a manual browser guide does not claim authentication", async () => {
    const guide = { channel: "twitter" as const, title: "Connect", url: "https://x.com/i/flow/login", instructions: [] };
    vi.mocked(api.beginSearchChannelAuth).mockResolvedValue(guide);
    expect(await connectSearchChannel("twitter", vi.fn())).toBeUndefined();
    expect(openUrl).toHaveBeenCalledWith(guide.url);
    expect(api.searchCapabilities).not.toHaveBeenCalled();
  });

  it("propagates a failed login without starting session polling", async () => {
    vi.mocked(api.beginSearchChannelAuth).mockRejectedValue(new Error("login timed out"));
    await expect(connectSearchChannel("facebook", vi.fn())).rejects.toThrow("login timed out");
    expect(api.searchCapabilities).not.toHaveBeenCalled();
  });

  it("never overlaps slow checks and stops immediately after a positive verdict", async () => {
    const first = deferred<SearchCapabilities>();
    vi.mocked(api.searchCapabilities).mockReturnValueOnce(first.promise).mockResolvedValue(capabilities(true));
    const onCheck = vi.fn();
    const pending = pollSearchChannelAuth("facebook", new AbortController().signal, onCheck, vi.fn());
    await vi.advanceTimersByTimeAsync(60_000);
    expect(api.searchCapabilities).toHaveBeenCalledTimes(1);
    first.resolve(capabilities(false));
    await vi.advanceTimersByTimeAsync(4_999);
    expect(api.searchCapabilities).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(await pending).toBe(true);
    expect(onCheck).toHaveBeenLastCalledWith(capabilities(true));
    expect(api.searchCapabilities).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("still reaches its deadline when every check throws", async () => {
    vi.mocked(api.searchCapabilities).mockRejectedValue(new Error("offline"));
    const onError = vi.fn();
    const pending = pollSearchChannelAuth("facebook", new AbortController().signal, vi.fn(), onError);
    await vi.advanceTimersByTimeAsync(300_000);
    expect(await pending).toBe(false);
    expect(onError).toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("ignores a late check after cancellation", async () => {
    const first = deferred<SearchCapabilities>();
    vi.mocked(api.searchCapabilities).mockReturnValue(first.promise);
    const controller = new AbortController();
    const onCheck = vi.fn();
    const pending = pollSearchChannelAuth("facebook", controller.signal, onCheck, vi.fn());
    controller.abort();
    first.resolve(capabilities(true));
    expect(await pending).toBe(false);
    expect(onCheck).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("clears the retry timer when waiting is cancelled", async () => {
    vi.mocked(api.searchCapabilities).mockResolvedValue(capabilities(false));
    const controller = new AbortController();
    const pending = pollSearchChannelAuth("facebook", controller.signal, vi.fn(), vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    controller.abort();
    expect(await pending).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("channel status labels", () => {
  it.each([
    ["login_required", "需连接"], ["bridge_required", "需连接浏览器"],
    ["check_timed_out", "检查超时"], ["check_failed", "检查失败"],
  ])("shows %s distinctly from a completed connection", (status, label) => {
    expect(channelState(capabilities(false, status).channels[0], false).label).toBe(label);
  });

  it("requires an authenticated verdict and retains the pending state", () => {
    const health: ChannelHealth = capabilities(true).channels[0];
    expect(channelState(health, false).label).toBe("已连接");
    expect(channelState(health, true).label).toBe("等待认证");
    expect(channelState({ ...health, available: false }, false).label).toBe("需准备");
  });
});
