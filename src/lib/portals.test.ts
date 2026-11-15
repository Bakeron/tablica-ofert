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
  const by = (portal: string[]) => applyFilters(rows, { ...NO_FILTERS, portal });

  it("a job with ads on several portals shows under each of them", () => {
    expect(rowPortals(rows[1])).toEqual(["nfj", "pracuj", "praca.pl"]);
    expect(by(["pracuj"])).toEqual([rows[0], rows[1]]);
    expect(by(["praca.pl"])).toEqual([rows[1]]);
    expect(by(["nfj"])).toEqual([rows[1], rows[2]]);
  });

  it("several portals ticked - an offer from any of them", () => {
    expect(by(["jjit", "manual"])).toEqual([rows[2], rows[3]]);
  });

  it("nothing ticked - the whole list", () => {
    expect(by([])).toBe(rows);
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

  it("a chosen value stays as an option with 0 on a list without it", () => {
    expect(filterCounts([row(["pracuj"])], { ...NO_FILTERS, portal: ["nfj"] }, "portal")).toEqual([
      { value: "pracuj", count: 1 },
      { value: "nfj", count: 0 },
    ]);
  });
});

describe("status, work mode, city, salary and English filters", () => {
  const a = row(["pracuj"], [], { stage: "applied", city: "Kraków", remote: "hybrid", salaryMin: 12000, salaryPeriod: "month" });
  const b = row(["nfj"], [], { stage: "new", remote: "yes", salaryMin: 120, salaryMax: 150, salaryPeriod: "hour" });
  const c = row(["jjit"], [], { stage: "new", city: "Gdańsk", remote: "no" });
  const rows = [a, b, c];

  it("status is the stage the list shows, in the app's order", () => {
    expect(applyFilters(rows, { ...NO_FILTERS, status: ["new"] })).toEqual([b, c]);
    expect(filterCounts(rows, NO_FILTERS, "status")).toEqual([
      { value: "new", count: 2 },
      { value: "applied", count: 1 },
    ]);
  });

  it("work mode and city are separate, each with several values at once", () => {
    expect(applyFilters(rows, { ...NO_FILTERS, city: ["Kraków"] })).toEqual([a]);
    expect(applyFilters(rows, { ...NO_FILTERS, mode: ["yes", "no"] })).toEqual([b, c]);
    expect(applyFilters(rows, { ...NO_FILTERS, mode: ["yes", "hybrid"], city: ["Kraków"] })).toEqual([a]);
    expect(filterCounts(rows, NO_FILTERS, "mode").map((x) => x.value)).toEqual(["yes", "hybrid", "no"]);
    expect(filterCounts(rows, NO_FILTERS, "city").map((x) => x.value)).toEqual(["Gdańsk", "Kraków"]);
  });

  it("salary: not given, per month, per day, per hour", () => {
    const d = row(["jjit"], [], { salaryMin: 1200, salaryMax: 1400, salaryPeriod: "day" });
    expect(applyFilters(rows, { ...NO_FILTERS, salary: ["none"] })).toEqual([c]);
    expect(applyFilters([...rows, d], { ...NO_FILTERS, salary: ["day", "hour"] })).toEqual([b, d]);
    expect(filterCounts([...rows, d], NO_FILTERS, "salary")).toEqual([
      { value: "none", count: 1 },
      { value: "month", count: 1 },
      { value: "day", count: 1 },
      { value: "hour", count: 1 },
    ]);
  });

  it("English: unticked hides offers that need it, ticked shows all; the count is what the box adds", () => {
    const en = [row(["nfj"], [], { english: "nice" }), row(["jjit"], [], { english: "required" }), row(["pracuj"])];
    expect(applyFilters(en, NO_FILTERS)).toEqual([en[0], en[2]]);
    expect(applyFilters(en, { ...NO_FILTERS, english: ["all"] })).toEqual(en);
    expect(filterCounts(en, NO_FILTERS, "english")).toEqual([{ value: "all", count: 1 }]);
    expect(filterCounts(en, { ...NO_FILTERS, portal: ["nfj"] }, "english")).toEqual([{ value: "all", count: 0 }]);
  });

  it("English: a family stays while one of its ads does not need it", () => {
    const fam = row(["jjit"], [["pracuj"]], { english: "required" });
    expect(applyFilters([fam], NO_FILTERS)).toEqual([fam]);
  });

  it("each filter counts on the list narrowed by the others, and keeps every value", () => {
    const f = { ...NO_FILTERS, status: ["new"] };
    expect(applyFilters(rows, { ...f, salary: ["none"] })).toEqual([c]);
    expect(filterCounts(rows, f, "salary").find((x) => x.value === "hour")).toEqual({ value: "hour", count: 1 });
    // Values the other filters leave out stay, with 0 - the filter never vanishes.
    expect(filterCounts(rows, { ...NO_FILTERS, status: ["applied"] }, "mode").find((x) => x.value === "yes")).toEqual({ value: "yes", count: 0 });
    // The status's own counts ignore the status chosen.
    expect(filterCounts(rows, f, "status").find((x) => x.value === "applied")).toEqual({ value: "applied", count: 1 });
  });
});
