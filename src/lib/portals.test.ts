import { describe, expect, it } from "vitest";
import { byPortal, portalCounts, rowPortals } from "./portals";

const row = (sources: string[], children: string[][] = []) => ({ sources, children: children.map((s) => ({ sources: s })) as never[] });

describe("portal filter", () => {
  const rows = [row(["pracuj"]), row(["nfj"], [["pracuj"], ["praca.pl"]]), row(["jjit", "nfj"]), row(["manual"])];

  it("a job with ads on several portals shows under each of them", () => {
    expect(rowPortals(rows[1])).toEqual(["nfj", "pracuj", "praca.pl"]);
    expect(byPortal(rows, "pracuj")).toEqual([rows[0], rows[1]]);
    expect(byPortal(rows, "praca.pl")).toEqual([rows[1]]);
    expect(byPortal(rows, "nfj")).toEqual([rows[1], rows[2]]);
  });

  it("no portal chosen - the whole list", () => {
    expect(byPortal(rows, null)).toBe(rows);
  });

  it("counts offers per portal, the biggest first", () => {
    expect(portalCounts(rows, null)).toEqual([
      { portal: "nfj", count: 2 },
      { portal: "pracuj", count: 2 },
      { portal: "jjit", count: 1 },
      { portal: "manual", count: 1 },
      { portal: "praca.pl", count: 1 },
    ]);
  });

  it("the chosen portal stays as an option with 0 on a list without it", () => {
    expect(portalCounts([row(["pracuj"])], "nfj")).toEqual([
      { portal: "pracuj", count: 1 },
      { portal: "nfj", count: 0 },
    ]);
  });
});
