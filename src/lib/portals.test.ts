import { describe, expect, it } from "vitest";
import { applyFilters, filterCounts, NO_FILTERS, rowPortals, rowValues } from "./portals";

type R = Parameters<typeof rowValues>[0];
const row = (sources: string[], children: string[][] = [], more: Partial<R> = {}): R =>
  ({
    sources,
    children: children.map((s) => ({ sources: s, city: null, remote: "unknown", english: "none" })) as never[],
    stage: "new",
    city: null,
    remote: "unknown",
    salaryMin: null,
    salaryMax: null,
    salaryPeriod: null,
    english: "none",
    ...more,
  }) as R;

describe("portal filter", () => {
  const rows = [row(["pracuj"]), row(["nfj"], [["pracuj"], ["praca.pl"]]), row(["jjit", "nfj"]), row(["manual"])];
  const by = (portal: string | null) => applyFilters(rows, { ...NO_FILTERS, portal });

  it("a job with ads on several portals shows under each of them", () => {
    expect(rowPortals(rows[1])).toEqual(["nfj", "pracuj", "praca.pl"]);
    expect(by("pracuj")).toEqual([rows[0], rows[1]]);
    expect(by("praca.pl")).toEqual([rows[1]]);
    expect(by("nfj")).toEqual([rows[1], rows[2]]);
  });

  it("no filter chosen - the whole list", () => {
    expect(by(null)).toBe(rows);
  });

  it("counts offers per portal, the biggest first", () => {
    expect(filterCounts(rows, NO_FILTERS, "portal")).toEqual([
      { value: "nfj", count: 2 },
      { value: "pracuj", count: 2 },
      { value: "jjit", count: 1 },
      { value: "manual", count: 1 },
      { value: "praca.pl", count: 1 },
    ]);
  });

  it("the chosen value stays as an option with 0 on a list without it", () => {
    expect(filterCounts([row(["pracuj"])], { ...NO_FILTERS, portal: "nfj" }, "portal")).toEqual([
      { value: "pracuj", count: 1 },
      { value: "nfj", count: 0 },
    ]);
  });
});

describe("status, place and salary filters", () => {
  const a = row(["pracuj"], [], { stage: "applied", city: "Kraków", remote: "hybrid", salaryMin: 12000, salaryPeriod: "month" });
  const b = row(["nfj"], [], { stage: "new", remote: "yes", salaryMin: 120, salaryMax: 150, salaryPeriod: "hour" });
  const c = row(["jjit"], [], { stage: "new", city: "Gdańsk", remote: "no" });
  const rows = [a, b, c];

  it("status is the stage the list shows, in the app's order", () => {
    expect(applyFilters(rows, { ...NO_FILTERS, status: "new" })).toEqual([b, c]);
    expect(filterCounts(rows, NO_FILTERS, "status")).toEqual([
      { value: "new", count: 2 },
      { value: "applied", count: 1 },
    ]);
  });

  it("place: work modes first, then cities", () => {
    expect(applyFilters(rows, { ...NO_FILTERS, place: "city:Kraków" })).toEqual([a]);
    expect(applyFilters(rows, { ...NO_FILTERS, place: "remote:yes" })).toEqual([b]);
    expect(filterCounts(rows, NO_FILTERS, "place").map((x) => x.value)).toEqual(["remote:yes", "remote:hybrid", "remote:no", "city:Gdańsk", "city:Kraków"]);
  });

  it("salary: given, not given, per month, per hour", () => {
    expect(applyFilters(rows, { ...NO_FILTERS, salary: "any" })).toEqual([a, b]);
    expect(applyFilters(rows, { ...NO_FILTERS, salary: "none" })).toEqual([c]);
    expect(applyFilters(rows, { ...NO_FILTERS, salary: "hour" })).toEqual([b]);
    expect(filterCounts(rows, NO_FILTERS, "salary")).toEqual([
      { value: "any", count: 2 },
      { value: "none", count: 1 },
      { value: "month", count: 1 },
      { value: "hour", count: 1 },
    ]);
  });

  it("English: required, a plus, not named - in this order", () => {
    const en = [row(["nfj"], [], { english: "nice" }), row(["jjit"], [], { english: "required" }), row(["pracuj"])];
    expect(applyFilters(en, { ...NO_FILTERS, english: "required" })).toEqual([en[1]]);
    expect(filterCounts(en, NO_FILTERS, "english").map((x) => x.value)).toEqual(["required", "nice", "none"]);
  });

  it("a day rate is its own salary choice", () => {
    const d = row(["jjit"], [], { salaryMin: 1200, salaryMax: 1400, salaryPeriod: "day" });
    expect(applyFilters([...rows, d], { ...NO_FILTERS, salary: "day" })).toEqual([d]);
  });

  it("each filter counts on the list narrowed by the others", () => {
    const f = { ...NO_FILTERS, status: "new" };
    expect(applyFilters(rows, { ...f, salary: "none" })).toEqual([c]);
    expect(filterCounts(rows, f, "salary").find((x) => x.value === "any")).toEqual({ value: "any", count: 1 });
    // The status's own counts ignore the status chosen.
    expect(filterCounts(rows, f, "status").find((x) => x.value === "applied")).toEqual({ value: "applied", count: 1 });
  });
});
