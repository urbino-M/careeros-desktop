import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => { vi.unstubAllGlobals(); vi.resetModules(); });

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); } };
}

describe("interface preferences", () => {
  it("restores language/theme, updates the document, and persists both independently", async () => {
    const storage = memoryStorage({"careeros-locale":"en", "careeros-theme":"light"});
    const element = { lang: "", dataset: {} as Record<string, string> };
    const meta = { setAttribute: vi.fn() };
    vi.stubGlobal("localStorage", storage);
    vi.stubGlobal("document", { documentElement: element, querySelector: () => meta });
    const {getUiPreferences, setUiPreferences} = await import("./uiPreferences");
    expect(getUiPreferences()).toEqual({locale:"en",theme:"light"});
    expect(element).toEqual({lang:"en",dataset:{theme:"light"}});
    setUiPreferences({theme:"dark"});
    expect(storage.getItem("careeros-locale")).toBe("en");
    expect(storage.getItem("careeros-theme")).toBe("dark");
    setUiPreferences({locale:"zh"});
    expect(element.lang).toBe("zh-CN");
    expect(element.dataset.theme).toBe("dark");
    expect(meta.setAttribute).toHaveBeenLastCalledWith("content", "#06171f");
  });

  it("still switches during a session when storage is blocked or invalid", async () => {
    vi.stubGlobal("localStorage", {getItem: () => {throw new Error("blocked");}, setItem: () => {throw new Error("blocked");}});
    const {getUiPreferences,setUiPreferences} = await import("./uiPreferences");
    expect(getUiPreferences()).toEqual({locale:"zh",theme:"dark"});
    expect(() => setUiPreferences({locale:"en",theme:"light"})).not.toThrow();
    expect(getUiPreferences()).toEqual({locale:"en",theme:"light"});
  });

  it("renders translated controls with accessible selected states", async () => {
    vi.stubGlobal("localStorage", memoryStorage());
    const {setUiPreferences} = await import("./uiPreferences");
    const {InterfacePreferences} = await import("./components/InterfacePreferences");
    setUiPreferences({locale:"en",theme:"light"});
    let html=renderToStaticMarkup(<InterfacePreferences />);
    expect(html).toContain('aria-label="Appearance"');
    expect(html).toContain('aria-label="Interface language"');
    expect(html).toMatch(/>\s*Light<\/button>/);
    expect(html.match(/aria-pressed="true"/g)).toHaveLength(2);
    setUiPreferences({locale:"zh",theme:"dark"});
    html=renderToStaticMarkup(<InterfacePreferences />);
    expect(html).toContain('aria-label="外观"');
    expect(html).toContain('黑暗');
  });

  it("translates explicit copy and positional values without rewriting user text", async () => {
    const {translate} = await import("./i18n");
    expect(translate("  设置  ","en")).toBe("  Settings  ");
    expect(translate("自定义研究方向：中文原文", "en")).toBe("自定义研究方向：中文原文");
    expect(translate("未知键 {0}", "en", "不要改我的CV")).toBe("未知键 不要改我的CV");
  });
});
