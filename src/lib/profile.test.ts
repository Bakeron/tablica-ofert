import { describe, expect, it } from "vitest";
import { translate } from "../i18n";
import { bestResult, emptyCriteria, reasonText, sortedReasons, splitList, toAmount, viewFor } from "./profile";

const t = (key: Parameters<typeof translate>[1], params?: Record<string, string | number>) => translate("pl", key, params);

describe("profile helpers", () => {
  it("splits lists by commas, semicolons and lines without repeats", () => {
    expect(splitList("React, TypeScript; react\nNode.js ,, ")).toEqual(["React", "TypeScript", "Node.js"]);
    expect(splitList("   ")).toEqual([]);
  });

  it("reads amounts typed in any common way", () => {
    expect(toAmount("12 000")).toBe(12000);
    expect(toAmount("12 500")).toBe(12500);
    expect(toAmount("62,5")).toBe(63);
    expect(toAmount("")).toBeNull();
    expect(toAmount("-5")).toBeNull();
    expect(toAmount("abc")).toBeNull();
  });

  it("new profile: keywords only feed the score, the rest are hard filters", () => {
    const c = emptyCriteria();
    expect(c.hard.keywords).toBe(false);
    expect(c.hard.location && c.hard.salary && c.hard.contract).toBe(true);
    expect(c.salaryUop.basis).toBe("gross");
    expect(c.salaryB2b.basis).toBe("net");
  });

  it("describes reasons in plain words", () => {
    expect(reasonText({ kind: "fail", code: "contract_mismatch", value: "B2B" }, t)).toBe("umowa: B2B");
    expect(reasonText({ kind: "unsure", code: "remote_unknown" }, t)).toBe("brak informacji o pracy zdalnej");
    expect(reasonText({ kind: "note", code: "something_new" }, t)).toBe("something_new");
    const sorted = sortedReasons([
      { kind: "note", code: "no_salary" },
      { kind: "unsure", code: "level_unknown" },
      { kind: "fail", code: "salary_low" },
    ]);
    expect(sorted.map((r) => r.kind)).toEqual(["fail", "unsure", "note"]);
  });

  it("puts offers in the right tab", () => {
    const base = { stage: "new" as const, favorite: false, interest: null };
    expect(viewFor({ ...base, filterResult: "pass" })).toBe("board");
    expect(viewFor({ ...base, filterResult: "unsure" })).toBe("review");
    expect(viewFor({ ...base, filterResult: "fail" })).toBe("rejected");
    expect(viewFor({ ...base, filterResult: "fail", interest: "high" })).toBe("board");
    expect(viewFor({ ...base, filterResult: "fail", interest: "no" })).toBe("declined");
    expect(viewFor({ ...base, filterResult: "pass", interest: "no", favorite: true })).toBe("declined");
    expect(viewFor({ ...base, filterResult: "unsure", stage: "applied" })).toBe("board");
  });

  it("one fitting profile is enough", () => {
    expect(bestResult([])).toBe("pass");
    expect(bestResult(["fail", "unsure"])).toBe("unsure");
    expect(bestResult(["fail", "pass"])).toBe("pass");
    expect(bestResult(["fail"])).toBe("fail");
  });
});
