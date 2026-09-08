import { describe, expect, it, vi } from "vitest";
import { openPath } from "@tauri-apps/plugin-opener";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { TargetDetail } from "../types";
import { setUiPreferences } from "../uiPreferences";
import { translate } from "../i18n";
import { detailMessages } from "../i18n/detail";
import {
  linkifyBareUrls,
  parseEmailMarkdown,
  parseLetter,
  parseMarkdownBlocks,
  parseRevisionDiff,
  revealArtifactInFinder,
  ManualContactActions,
  pdfPreviewSha256,
  gmailDraftNotice,
  ReplyRevisionLink,
  MaterialRecoveryPanel,
} from "./ApplicationDetailPage";

describe("incomplete material recovery", () => {
  const detail: TargetDetail = {
    target: {id:"target",applicationId:"app",opportunityId:"opportunity",name:"Contact",organization:"University",title:"Research role",priority:0,status:"ready_to_contact",submissionStatus:"not_set",materialStatus:"pending",careerTrack:"postdoc",updatedAt:"2026-09-07"},
    artifacts:[],checklist:[],replies:[],revisions:[],
    recoveryJob:{id:"original",jobType:"full_search",resultTargetIds:["target"],status:"needs_review",progress:90,providerId:"openai",createdAt:"2026-09-06",events:[]},
    unpublishedCv:[{artifactType:"cv_pdf",language:"en",path:"/test/candidate/cv.pdf",exists:true,updatedAt:"2026-09-07"}],
  };
  const render=(value:TargetDetail)=>renderToStaticMarkup(createElement(MaterialRecoveryPanel,{detail:value,onChanged:()=>{},onNavigate:()=>{}}));
  it("offers original-task and scoped recovery without an official CV",()=>{
    const html=render(detail);
    expect(html).toContain("打开原任务继续修复");
    expect(html).toContain("只补齐这条机会（新任务）");
    expect(html).toContain("不可作为已审核附件");
    expect(html).toContain("查看候选 PDF");
  });
  it("blocks recovery while shelved and explains missing associations",()=>{
    const shelved=render({...detail,target:{...detail.target,status:"shelved"}});
    expect(shelved).toContain("请先恢复");
    expect(shelved).not.toContain("打开原任务继续修复");
    const orphan=render({...detail,recoveryJob:null,target:{...detail.target,opportunityId:undefined}});
    expect(orphan).toContain("缺少关联机会");
    expect(orphan).not.toContain("只补齐这条机会（新任务）");
  });
});

describe("reviewed PDF and draft outcomes", () => {
  it("hashes the actual preview bytes, not a later file read", async () => {
    expect(await pdfPreviewSha256(btoa("abc"))).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  });
  it("distinguishes a pending remote check from a failed creation", () => {
    expect(gmailDraftNotice({ gmailDraftId: "draft-1", remoteVerified: false })).toContain("只核验");
    expect(gmailDraftNotice({ gmailDraftId: "unconfirmed:request", remoteVerified: false })).not.toContain("已创建");
  });
});

vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), openUrl: vi.fn(), revealItemInDir: vi.fn() }));

describe("application detail UI language", () => {
  it("translates contact controls and dynamic Gmail feedback without changing identifiers", () => {
    try {
      setUiPreferences({locale:"en"});
      const html = renderToStaticMarkup(createElement(ManualContactActions, {status:"contacted",busy:false,onChange:()=>{},onReply:()=>{}}));
      expect(html).toContain("Manage contact progress");
      expect(html).toContain("Mark as replied");
      expect(html).not.toContain("标记已回复");
      expect(gmailDraftNotice({gmailDraftId:"draft-abc",remoteVerified:true})).toBe("Gmail draft created and remotely verified: draft-abc. Not sent.");
      expect(parseLetter("收件人：张教授\n主题：原始主题\n\n请保留原始正文。","zh")).toMatchObject({to:"张教授",subject:"原始主题",paragraphs:["请保留原始正文。"]});
    } finally { setUiPreferences({locale:"zh"}); }
  });
  it("registers every detail message with the shared translator", () => {
    for (const [source,english] of Object.entries(detailMessages)) {
      expect(translate(source,"en")).toBe(english);
      expect(translate(source,"zh")).toBe(source);
    }
  });
});

describe("reply result history", () => {
  it("opens the selected historical result, not the current draft", () => {
    const path = "/test/generated/reply-history/previous/followup-email-en.md";
    for (const type of ["reply_analysis", "followup_email"]) {
      const link = ReplyRevisionLink({ type, path });
      expect(renderToStaticMarkup(link!)).toContain("查看本次回复结果");
      link!.props.onClick();
      expect(openPath).toHaveBeenLastCalledWith(path);
    }
    expect(ReplyRevisionLink({ type: "cv_data", path })).toBeNull();
  });
});

describe("manual contact decisions", () => {
  it("offers reply and shelving without requiring reply text at every active stage", () => {
    for (const status of ["ready_to_contact", "contacted", "replied", "follow_up"] as const) {
      const html = renderToStaticMarkup(createElement(ManualContactActions, { status, busy: false, onChange: () => {}, onReply: () => {} }));
      expect(html).toContain("录入 / 处理回复");
      expect(html).toContain("搁置");
      expect(html).toContain("不影响同一机会下的其他人");
      if (status !== "replied") expect(html).toContain("标记已回复");
    }
  });
  it("offers recovery from shelving and disables actions while saving", () => {
    const html = renderToStaticMarkup(createElement(ManualContactActions, { status: "shelved", busy: true, onChange: () => {}, onReply: () => {} }));
    expect(html).toContain("移回跟进");
    expect(html).toContain("移回待处理");
    expect(html).toContain('disabled=""');
    expect(html).not.toContain(">搁置</button>");
  });
  it("routes marking replied separately from recording a reply", () => {
    const changes: string[] = [];
    let opened = false;
    const tree = ManualContactActions({ status: "ready_to_contact", busy: false, onChange: (status) => changes.push(status), onReply: () => { opened = true; } });
    const buttons = tree.props.children.filter((child: { type?: string }) => child && child.type === "button");
    buttons.find((child: { props: { children: string } }) => child.props.children === "标记已回复").props.onClick();
    expect(changes).toEqual(["replied"]);
    expect(opened).toBe(false);
    buttons.find((child: { props: { children: string } }) => child.props.children === "录入 / 处理回复").props.onClick();
    expect(opened).toBe(true);
  });
});

describe("application material formatting", () => {
  it("extracts plain-text email headers for Gmail without leaking them into the body", () => {
    const parsed = parseEmailMarkdown("To: pi@example.edu\nSubject: Research fit\n\nDear Professor,\n\nHello.");
    expect(parsed.subject).toBe("Research fit");
    expect(parsed.body).toBe("Dear Professor,\n\nHello.");
  });

  it("drops a legacy bare recipient line from the Gmail body", () => {
    const parsed = parseEmailMarkdown("pi@example.edu\n\nSubject: Research fit\n\nDear Professor,\n\nHello.");
    expect(parsed.subject).toBe("Research fit");
    expect(parsed.body).toBe("Dear Professor,\n\nHello.");
  });

  it("accepts full-width separators in legacy email headers", () => {
    const parsed = parseEmailMarkdown("To：pi@example.edu\nSubject：Research fit\n\nDear Professor,");
    expect(parsed.subject).toBe("Research fit");
    expect(parsed.body).toBe("Dear Professor,");
  });

  it("splits Chinese letters into metadata and readable paragraphs", () => {
    const parsed = parseLetter("主题：合作机会\n\n收件人：contact@example.edu\n\n您好：\n\n正文。", "zh");
    expect(parsed.subject).toBe("合作机会");
    expect(parsed.to).toBe("contact@example.edu");
    expect(parsed.paragraphs).toEqual(["您好：", "正文。"]);
  });

  it("turns GFM-style score tables into structured report blocks", () => {
    const blocks = parseMarkdownBlocks("# Fit\n\n| Lane | Score |\n| --- | ---: |\n| Methods | 24/25 |");
    expect(blocks).toHaveLength(2);
    expect(blocks[1]).toMatchObject({
      kind: "table",
      headers: ["Lane", "Score"],
      rows: [["Methods", "24/25"]],
    });
  });

  it("makes bare evidence URLs valid Markdown autolinks", () => {
    expect(linkifyBareUrls("Source: https://example.edu/profile")).toBe("Source: <https://example.edu/profile>");
  });

  it("turns revision protocol JSON into exact before-and-after rows", () => {
    expect(parseRevisionDiff('[{"line":"22","before":"old claim","after":"new evidence"}]')).toEqual([
      { line: 22, before: "old claim", after: "new evidence" },
    ]);
    expect(parseRevisionDiff("not-json")).toEqual([]);
  });

  it("reveals the exact current CV file in Finder", async () => {
    const revealed: string[] = [];
    await revealArtifactInFinder("/materials/current/cv.pdf", async (path) => { revealed.push(path); });
    expect(revealed).toEqual(["/materials/current/cv.pdf"]);
  });
});
