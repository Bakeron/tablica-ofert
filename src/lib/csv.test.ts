import { describe, expect, it } from "vitest";
import type { ExportRow } from "../api/types";
import { CSV_LISTS, csvCell, csvCells, csvDate, csvFileName, csvName, csvSeparator, csvText } from "./csv";

const row: ExportRow = {
  id: 1,
  title: "Programista Java",
  company: "Firma Testowa",
  city: "Wrocław",
  remote: "hybrid",
  salaryMin: 9000,
  salaryMax: null,
  salaryCurrency: "PLN",
  salaryPeriod: "month",
  contract: "uop",
  level: "mid",
  stage: "applied",
  interest: "high",
  favorite: true,
  score: 72,
  scoreSource: "keywords",
  sources: ["jjit", "jooble", "nieznane"],
  url: "https://example.com/oferta",
  addedAt: "2026-11-05T18:14:00Z",
  notes: null,
  list: "review",
  publishedAt: "2026-11-04T08:00:00Z",
};

describe("csv", () => {
  it("separates columns the way Excel reads them in the app's language", () => {
    expect(csvSeparator("pl")).toBe(";");
    expect(csvSeparator("en")).toBe(",");
  });

  it("turns codes into words of the app language", () => {
    const [head, cells] = csvCells([row], "pl");
    expect(head).toHaveLength(cells.length);
    const at = (name: string) => cells[head.indexOf(name)];
    expect(at("Praca zdalna")).toBe("hybrydowo");
    expect(at("Etap")).toBe("Wysłano");
    expect(at("Okres")).toBe("miesięcznie");
    expect(at("Ulubiona")).toBe("tak");
    expect(at("Źródła")).toBe("JJIT, Jooble, nieznane");
    expect(at("Lista")).toBe("Zebrane - Do sprawdzenia");
    expect(at("Widełki do")).toBe("");
    expect(at("Opublikowano")).toMatch(/^2026-11-04 \d\d:00$/);
    expect(csvCells([row], "en")[1][3]).toBe("hybrid");
  });

  it("writes the date the way Excel reads it", () => {
    expect(csvDate("2026-11-05T18:14:00Z")).toMatch(/^2026-11-0[56] \d\d:14$/);
    expect(csvDate("nie data")).toBe("nie data");
  });

  it("names every list in a file-safe way", () => {
    for (const lang of ["pl", "en"] as const) {
      for (const list of CSV_LISTS) expect(csvName(list, lang)).toMatch(/^[a-z0-9-]{1,40}$/);
    }
  });

  it("writes the demo file like Rust: BOM, quotes, formulas made safe", () => {
    expect(csvCell('Firma "Cudzysłów"')).toBe('"Firma ""Cudzysłów"""');
    expect(csvCell("=SUMA(A1)")).toBe(`"'=SUMA(A1)"`);
    expect(csvCell("-5")).toBe(`"'-5"`);
    expect(csvCell("x".repeat(40_000))).toHaveLength(32_002);
    expect(csvText([["a", "b"], ["ż", ""]], ";")).toBe('\uFEFF"a";"b"\r\n"ż";""\r\n');
  });

  it("names the demo file as the app does", () => {
    expect(csvFileName("oferty", new Date("2026-11-05T08:04:09.123Z"))).toBe("tablica-oferty-20261105-080409.csv");
  });
});
