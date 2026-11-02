import { describe, expect, it } from "vitest";
import { en } from "./en";
import { pl } from "./pl";
import { plural, resolveLang, translate } from "./index";

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("translations", () => {
  it("every Polish text has an English one and vice versa", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(pl).sort());
  });

  it("no text is empty and placeholders match", () => {
    for (const key of Object.keys(pl) as (keyof typeof pl)[]) {
      expect(pl[key].trim(), key).not.toBe("");
      expect(en[key].trim(), key).not.toBe("");
      expect(placeholders(en[key]), key).toEqual(placeholders(pl[key]));
    }
  });

  it("uses only the plain hyphen, never en or em dashes", () => {
    for (const text of [...Object.values(pl), ...Object.values(en)]) {
      expect(text).not.toMatch(/[–—]/);
    }
  });

  it("fills placeholders", () => {
    expect(translate("pl", "sources.many", { count: 3, list: "JJIT · NFJ · Pracuj" })).toBe(
      "na 3 portalach: JJIT · NFJ · Pracuj",
    );
  });

  it("uses Polish plural forms", () => {
    const count = (n: number) => translate("pl", plural("pl", "board.count", n), { count: n });
    expect([1, 2, 5, 22, 25].map(count)).toEqual(["1 oferta", "2 oferty", "5 ofert", "22 oferty", "25 ofert"]);
    expect(translate("en", plural("en", "board.count", 1), { count: 1 })).toBe("1 offer");
  });

  it("picks the language from settings or the system", () => {
    expect(resolveLang("system", "pl-PL")).toBe("pl");
    expect(resolveLang("system", "de-DE")).toBe("en");
    expect(resolveLang("en", "pl-PL")).toBe("en");
  });
});
