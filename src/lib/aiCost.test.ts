import { describe, expect, it } from "vitest";
import { claudeCostText, claudeOfferCost, localDay, plnText, topUpTyped, usdText } from "./aiCost";

describe("cost of Claude before „Rate all again”", () => {
  it("counts Sonnet, Opus twice as much, an unknown model as Opus", () => {
    expect(claudeOfferCost("claude-sonnet-5-5")).toBe(0.05);
    expect(claudeOfferCost("claude-opus-5-5")).toBe(0.1);
    expect(claudeOfferCost("some-new-model")).toBe(0.1);
    expect(claudeOfferCost("claude-haiku-5-5")).toBeLessThan(0.01);
  });

  it("rounds up to 10 grosze and never shows zero", () => {
    expect(claudeCostText(30, "claude-sonnet-5-5", "pl")).toBe("1,50");
    expect(claudeCostText(3, "claude-sonnet-5-5", "pl")).toBe("0,20");
    expect(claudeCostText(1, "claude-haiku-5-5", "en")).toBe("0.10");
  });
});

describe("top-up of the Claude account", () => {
  it("reads a typed sum the way the settings keep it", () => {
    expect(topUpTyped("5")).toBe("5");
    expect(topUpTyped(" 20,50 $")).toBe("20.50");
    expect(topUpTyped("$ 5.5")).toBe("5.5");
    expect(topUpTyped("10 USD")).toBe("10");
    expect(topUpTyped("")).toBe("");
    for (const wrong of ["0", "5.555", "abc", "-5", "10000.01", "5.", "1e3", "5,5,5"]) {
      expect(topUpTyped(wrong), wrong).toBeNull();
    }
  });

  it("shows the sum in PLN at the rough rate", () => {
    expect(plnText(5, "pl")).toBe("20,00");
    expect(plnText(20.5, "en")).toBe("82.00");
  });

  it("shows spending in dollars, under a cent not as zero", () => {
    expect(usdText(1.234, "pl")).toBe("1,23 $");
    expect(usdText(1.234, "en")).toBe("$1.23");
    expect(usdText(0, "pl")).toBe("0,00 $");
    expect(usdText(0.003, "pl")).toBe("< 0,01 $");
    expect(usdText(0.003, "en")).toBe("< $0.01");
    expect(usdText(-1, "en")).toBe("$0.00");
  });

  it("gives today's local day", () => {
    expect(localDay(new Date(2026, 10, 1, 23, 59))).toBe("2026-11-01");
  });
});
