import { describe, expect, it } from "vitest";
import {
  linkifyBareUrls,
  parseEmailMarkdown,
  parseLetter,
  parseMarkdownBlocks,
  parseRevisionDiff,
  revealArtifactInFinder,
} from "./ApplicationDetailPage";

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
    const parsed = parseLetter("主题：博士后合作\n\n收件人：pi@example.edu\n\n教授您好：\n\n正文。", "zh");
    expect(parsed.subject).toBe("博士后合作");
    expect(parsed.to).toBe("pi@example.edu");
    expect(parsed.paragraphs).toEqual(["教授您好：", "正文。"]); 
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
