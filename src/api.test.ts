import { describe, expect, it } from "vitest";
import { errorMessage } from "./api";

describe("connection diagnostics", () => {
  it("keeps relay errors distinct from OAuth errors", () => {
    const relay=errorMessage("无法连接模型中转站的 /responses 接口: error sending request");
    expect(relay).not.toContain("ChatGPT 登录");
    expect(relay).toContain("/responses");
    const oauth=errorMessage("Token exchange failed: error sending request for url (https://auth.openai.com/oauth/token)");
    expect(oauth).toContain("令牌交换未完成");
    expect(oauth).not.toContain("授权成功");
    expect(errorMessage("error sending request")).toBe("error sending request");
  });
});
