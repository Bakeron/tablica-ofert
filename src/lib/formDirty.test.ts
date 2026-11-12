import { describe, expect, it } from "vitest";
import { formDirty } from "./formDirty";

const EMPTY = { title: "", company: "", salaryMin: null as number | null };

describe("formDirty", () => {
  it("an untouched form closes without asking", () => {
    expect(formDirty({ ...EMPTY }, EMPTY, ["", ""])).toBe(false);
  });

  it("spaces alone in the link or the pasted page are not a change", () => {
    expect(formDirty({ ...EMPTY }, EMPTY, ["  ", "\n\t"])).toBe(false);
  });

  it("a typed field, link or pasted page asks before closing", () => {
    expect(formDirty({ ...EMPTY, title: "Magazynier" }, EMPTY, ["", ""])).toBe(true);
    expect(formDirty({ ...EMPTY, salaryMin: 0 }, EMPTY, ["", ""])).toBe(true);
    expect(formDirty({ ...EMPTY }, EMPTY, ["https://example.com/oferta/7", ""])).toBe(true);
    expect(formDirty({ ...EMPTY }, EMPTY, ["", "Opis stanowiska"])).toBe(true);
  });

  it("while saving nothing counts as unsaved", () => {
    expect(formDirty({ ...EMPTY, title: "Magazynier" }, EMPTY, ["x"], true)).toBe(false);
  });
});
