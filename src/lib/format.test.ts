import { describe, expect, it } from "vitest";
import { translate, type TextKey } from "../i18n";
import { addedLabel, isWebUrl, needsFollowUp, placeLabel, salaryFull, salaryShort, shownScore, signedPoints } from "./format";

const t = (key: TextKey, params?: Record<string, string | number>) => translate("pl", key, params);

describe("score learned from ratings", () => {
  it("adds the correction and stays between 0 and 100", () => {
    expect(shownScore({ score: 70, learned: 8 })).toBe(78);
    expect(shownScore({ score: 70, learned: -6 })).toBe(64);
    expect(shownScore({ score: 95, learned: 15 })).toBe(100);
    expect(shownScore({ score: 5, learned: -15 })).toBe(0);
  });

  it("gives no number to an offer without a score", () => {
    expect(shownScore({ score: null, learned: 8 })).toBeNull();
  });

  it("writes the sign, also with a plain minus", () => {
    expect(signedPoints(8)).toBe("+8");
    expect(signedPoints(-6)).toBe("-6");
  });
});

describe("salary", () => {
  it("shows monthly PLN in thousands", () => {
    expect(salaryShort({ salaryMin: 12000, salaryMax: 16000, salaryCurrency: "PLN", salaryPeriod: "month" }, "pl", t)).toBe("12-16 tys");
    expect(salaryShort({ salaryMin: 9500, salaryMax: null, salaryCurrency: "PLN", salaryPeriod: "month" }, "pl", t)).toBe("9,5 tys");
    expect(salaryShort({ salaryMin: 9500, salaryMax: null, salaryCurrency: "PLN", salaryPeriod: "month" }, "en", (k) => translate("en", k))).toBe("9.5 k");
  });

  it("shows hourly rates and foreign currency", () => {
    expect(salaryShort({ salaryMin: 45, salaryMax: 60, salaryCurrency: "EUR", salaryPeriod: "hour" }, "pl", t)).toBe("45-60 EUR/h");
  });

  it("marks a missing salary instead of hiding the offer", () => {
    expect(salaryShort({ salaryMin: null, salaryMax: null, salaryCurrency: null, salaryPeriod: null }, "pl", t)).toBe("-");
    expect(salaryFull({ salaryMin: null, salaryMax: null, salaryCurrency: null, salaryPeriod: null }, "pl", t)).toBe("brak widełek");
  });
});

describe("place", () => {
  it("combines city and remote work", () => {
    expect(placeLabel({ city: null, remote: "yes" }, t)).toBe("zdalnie");
    expect(placeLabel({ city: "Wrocław", remote: "hybrid" }, t)).toBe("Wrocław · hybrydowo");
    expect(placeLabel({ city: "Łódź", remote: "unknown" }, t)).toBe("Łódź");
  });
});

describe("dates", () => {
  const now = new Date(2026, 10, 2, 12, 0);
  it("shows time today, 'wczoraj' yesterday, a date before", () => {
    expect(addedLabel(new Date(2026, 10, 2, 9, 12).toISOString(), "pl", t, now)).toBe("09:12");
    expect(addedLabel(new Date(2026, 10, 1, 20, 0).toISOString(), "pl", t, now)).toBe("wczoraj");
    expect(addedLabel(new Date(2026, 9, 28, 20, 0).toISOString(), "pl", t, now)).toBe("28.10");
  });

  it("asks for a follow-up 7 days after applying", () => {
    const sent = new Date(2026, 9, 27, 12, 0).toISOString(); // 6 days before `now`
    expect(needsFollowUp({ stage: "applied", stageChangedAt: sent }, now.getTime())).toBe(false);
    expect(needsFollowUp({ stage: "applied", stageChangedAt: sent }, now.getTime() + 86_400_000)).toBe(true);
    expect(needsFollowUp({ stage: "hr_interview", stageChangedAt: sent }, now.getTime() + 86_400_000)).toBe(false);
  });
});

describe("links", () => {
  it("opens only http and https", () => {
    expect(isWebUrl("https://example.com/a")).toBe(true);
    expect(isWebUrl("http://example.com")).toBe(true);
    expect(isWebUrl("javascript:alert(1)")).toBe(false);
    expect(isWebUrl("file:///C:/Windows")).toBe(false);
    expect(isWebUrl("not a url")).toBe(false);
    expect(isWebUrl(null)).toBe(false);
  });
});
