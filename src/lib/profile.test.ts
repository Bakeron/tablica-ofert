import { describe, expect, it } from "vitest";
import { translate } from "../i18n";
import { AMOUNT_PATTERN, bestResult, emptyCriteria, isBin, leavesView, placeName, reasonText, sortedReasons, splitList, toAmount, viewFor } from "./profile";

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
    expect(toAmount("12.000")).toBe(12000);
    expect(toAmount("1,200,000")).toBe(1200000);
    expect(toAmount("12.5")).toBe(13);
    expect(new RegExp(`^(?:${AMOUNT_PATTERN})$`, "v").test("12 000,50")).toBe(true);
    expect(new RegExp(`^(?:${AMOUNT_PATTERN})$`, "v").test("15k")).toBe(false);
    expect(new RegExp(`^(?:${AMOUNT_PATTERN})$`, "v").test("-5000")).toBe(false);
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
    const base = { stage: "new" as const, favorite: false, interest: null, own: false };
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

  it("knows when a change takes an offer out of its tab", () => {
    const offer = { stage: "new" as const, favorite: false, interest: null, filterResult: "pass" as const, expired: false, own: false };
    // "Today" keeps "very" and "maybe"; "no", a recruitment and "expired" take it away.
    expect(leavesView({ ...offer, interest: "high" }, "today")).toBeNull();
    expect(leavesView({ ...offer, interest: "no" }, "today")).toBe("declined");
    expect(leavesView({ ...offer, stage: "applied" }, "today")).toBe("board");
    expect(leavesView({ ...offer, expired: true }, "today")).toBe("board");
    // A filtered-out offer stays in "Today" only while it is kept.
    expect(leavesView({ ...offer, filterResult: "fail", favorite: true }, "today")).toBeNull();
    expect(leavesView({ ...offer, filterResult: "fail" }, "today")).toBe("rejected");
    // The other tabs as before; the archive and the trash keep theirs.
    expect(leavesView({ ...offer, filterResult: "unsure", interest: "high" }, "review")).toBe("board");
    // "All" holds "to check" too - an unsure offer without a mark stays there.
    expect(leavesView({ ...offer, filterResult: "unsure", interest: null }, "board")).toBeNull();
    expect(leavesView({ ...offer, interest: "no" }, "archive")).toBeNull();
    // "Own" keeps its offers whatever the filter or the stage says; only "no" takes one away.
    expect(viewFor({ ...offer, own: true, filterResult: "fail" })).toBe("own");
    expect(leavesView({ ...offer, own: true, stage: "applied", filterResult: "fail" }, "own")).toBeNull();
    expect(leavesView({ ...offer, own: true, interest: "no" }, "own")).toBe("declined");
    // A collected offer that became own (a stage, "To own") leaves every list of "Collected".
    expect(leavesView({ ...offer, own: true, stage: "interested" }, "today")).toBe("own");
    expect(leavesView({ ...offer, own: true, stage: "applied" }, "board")).toBe("own");
  });

  it("the bin keeps its offers until restored; a not-for-me one leaves when the interest changes", () => {
    const offer = { stage: "new" as const, favorite: false, interest: null, filterResult: "pass" as const, expired: false, own: false };
    expect(["bin", "declined", "archive", "trash"].every((v) => isBin(v as never))).toBe(true);
    expect(isBin("own") || isBin("board")).toBe(false);
    for (const view of ["bin", "declined"] as const) {
      expect(leavesView({ ...offer, interest: "no", bin: "declined" }, view)).toBeNull();
      expect(leavesView({ ...offer, interest: "high", bin: "declined" }, view)).toBe("board");
      expect(leavesView({ ...offer, own: true, interest: null, bin: "declined" }, view)).toBe("own");
    }
    // Expired and deleted ones stay whatever changes on the card.
    expect(leavesView({ ...offer, interest: "high", bin: "deleted" }, "bin")).toBeNull();
    expect(leavesView({ ...offer, stage: "applied", bin: "expired" }, "archive")).toBeNull();
  });

  it("names the bin's lists like the collected ones", () => {
    expect(placeName("bin", t)).toBe("Kosz");
    expect(placeName("declined", t)).toBe("Kosz - Nie dla mnie");
    expect(placeName("archive", t)).toBe("Kosz - Wygasłe");
    expect(placeName("trash", t)).toBe("Kosz - Usunięte");
    expect(placeName("board", t)).toBe("Zebrane - Wszystkie");
  });
});
