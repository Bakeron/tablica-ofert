import { describe, expect, it } from "vitest";
import type { OfferDetail, OfferInput } from "../api/types";
import { lostFields } from "./savedCheck";

const input: OfferInput = {
  title: " Specjalista ds. zamówień ",
  company: "Przykładowa Hurtownia",
  source: "manual",
  url: "https://example.com/oferta/1",
  city: "Poznań",
  remote: "hybrid",
  salaryMin: 8000,
  salaryMax: 6500,
  salaryCurrency: "pln",
  salaryPeriod: "month",
  contract: "UoP",
  level: "",
  description: "Opis oferty",
  publishedAt: "2026-11-10T09:00:00Z",
  expiresAt: "2026-12-04T21:59:59Z",
};

// Only the fields the check reads; the rest of the detail does not matter here.
const saved = {
  title: "Specjalista ds. zamówień",
  company: "Przykładowa Hurtownia",
  url: "https://example.com/oferta/1",
  city: "Poznań",
  remote: "hybrid",
  salaryMin: 6500,
  salaryMax: 8000,
  salaryCurrency: "PLN",
  salaryPeriod: "month",
  contract: "UoP",
  level: null,
  description: "Opis oferty",
  publishedAt: "2026-11-10T09:00:00Z",
  expiresAt: "2026-12-04T21:59:59Z",
} as unknown as OfferDetail;

describe("checking a new offer after saving", () => {
  it("accepts what the app saves by its own rules (trimmed, amounts in order, PLN)", () => {
    expect(lostFields(input, saved)).toEqual([]);
  });

  it("names a field lost or changed on the way", () => {
    const changed = { ...saved, city: null, salaryMin: 9000, contract: "B2B" } as OfferDetail;
    expect(lostFields(input, changed)).toEqual(["city", "salary", "contract"]);
  });

  it("checks the dates, a future publication date is saved as now", () => {
    expect(lostFields(input, { ...saved, expiresAt: null } as OfferDetail)).toEqual(["expires"]);
    expect(lostFields(input, { ...saved, publishedAt: "2026-11-11T12:00:00Z" } as OfferDetail)).toEqual(["published"]);
    const future: OfferInput = { ...input, publishedAt: "2999-01-01T00:00:00Z" };
    expect(lostFields(future, { ...saved, publishedAt: "2026-11-11T12:00:00Z" } as OfferDetail)).toEqual([]);
  });

  it("an empty salary stays empty, without a currency", () => {
    const none: OfferInput = { ...input, salaryMin: null, salaryMax: null };
    const plain = { ...saved, salaryMin: null, salaryMax: null, salaryCurrency: null, salaryPeriod: null } as OfferDetail;
    expect(lostFields(none, plain)).toEqual([]);
    expect(lostFields(none, saved)).toEqual(["salary"]);
  });
});
