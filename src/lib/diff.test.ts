import { describe, expect, it } from "vitest";
import { droppedWords, wordDiff } from "./diff";

describe("word diff for “Tidy up”", () => {
  it("marks removed and added words", () => {
    const parts = wordDiff("Szukam pracy zdalnej, bez sprzedaży", "- Praca zdalna\n- Bez sprzedaży");
    const removed = parts.filter((p) => p.kind === "removed").map((p) => p.text.trim());
    const added = parts.filter((p) => p.kind === "added").map((p) => p.text.trim());
    expect(removed.join(" ")).toContain("Szukam");
    expect(added.join(" ")).toContain("zdalna");
    // Same word with other case and punctuation is not a change.
    expect(parts.some((p) => p.kind === "same" && p.text.includes("sprzedaży"))).toBe(true);
  });

  it("joins the text back without losing anything", () => {
    const after = "- React\n- TypeScript";
    const parts = wordDiff("React i TypeScript", after);
    expect(parts.filter((p) => p.kind !== "removed").map((p) => p.text).join("")).toBe(after);
  });

  it("lists words AI dropped", () => {
    expect(droppedWords("React, TypeScript, bez dyżurów w nocy", "- React\n- TypeScript")).toEqual(["bez", "dyżurów", "nocy"]);
    expect(droppedWords("C++ i C#", "C++, C#")).toEqual([]);
  });
});
