import { describe, expect, it } from "vitest";
import type { RateReport } from "../api/types";
import { translate, type TextKey } from "../i18n";
import { rateSummary } from "./aiProblem";

const t = (key: TextKey, params?: Record<string, string | number>) => translate("pl", key, params);
const base: RateReport = { rated: 0, failed: 0, waiting: 36, noProfile: 0, error: null, problems: [] };

describe("rateSummary", () => {
  it("names the provider, the model and the reason", () => {
    const report: RateReport = {
      ...base,
      error: { code: "net", detail: "429: limit: 0" },
      problems: [{ provider: "gemini", model: "gemini-2.5-flash", kind: "model", message: "limit: 0" }],
    };
    expect(rateSummary(report, t)).toBe(
      "AI przerwało ocenianie. Google Gemini (gemini-2.5-flash): model nie istnieje albo jest niedostępny dla tego konta - wybierz inny darmowy model.",
    );
  });

  it("tells about a provider that failed even when the next one took over", () => {
    const report: RateReport = {
      ...base,
      rated: 3,
      waiting: 33,
      noProfile: 2,
      problems: [{ provider: "gemini", model: "x", kind: "key", message: "" }],
    };
    const text = rateSummary(report, t);
    expect(text.startsWith("AI oceniło oferty: 3, czeka jeszcze: 33. Google Gemini (x): klucz nieprawidłowy")).toBe(true);
    expect(text.endsWith("Pominięte oferty bez pasującego profilu: 2 (np. profil wyłączony).")).toBe(true);
  });

  it("keeps the short sentence when all went well", () => {
    expect(rateSummary({ ...base, rated: 5, waiting: 0 }, t)).toBe("AI oceniło oferty: 5, czeka jeszcze: 0");
  });
});
