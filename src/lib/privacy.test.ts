import { describe, expect, it } from "vitest";
import raw from "../../PRIVACY.md?raw";
import { inline, privacyBlocks, type Block } from "./privacy";

const plain = (blocks: Block[]) =>
  blocks
    .flatMap((b) => (b.kind === "list" ? b.items.flat() : b.parts))
    .map((p) => p.text)
    .join(" ");

describe("privacy policy in the app", () => {
  it("shows each language only in its own version", () => {
    const pl = plain(privacyBlocks(raw, "pl"));
    const en = plain(privacyBlocks(raw, "en"));
    expect(pl).toContain("Jakie dane i gdzie");
    expect(pl).not.toContain("What data and where");
    expect(en).toContain("What data and where");
    expect(en).not.toContain("Jakie dane i gdzie");
  });

  it("leaves no Markdown marks in the text", () => {
    for (const lang of ["pl", "en"] as const) {
      const text = plain(privacyBlocks(raw, lang));
      expect(text).not.toMatch(/\*\*|^#|\]\(|^---/m);
      expect(text).not.toContain("English version below");
    }
  });

  it("reads bold, code and only web links", () => {
    expect(inline("a **b** `c`")).toEqual([
      { kind: "text", text: "a " },
      { kind: "strong", text: "b" },
      { kind: "text", text: " " },
      { kind: "code", text: "c" },
    ]);
    expect(inline("[x](https://example.com)")).toEqual([{ kind: "link", text: "x", url: "https://example.com" }]);
    expect(inline("[x](javascript:alert(1))")[0]).toMatchObject({ kind: "text", text: "x" });
  });
});
