import { describe, expect, it } from "vitest";
import { sourceProblem } from "./sourceProblem";

describe("sourceProblem", () => {
  it("reads the kind from the saved error text", () => {
    expect(sourceProblem("network: 503 Service Unavailable: Uh oh, something isn't right")).toBe("down");
    expect(sourceProblem("network: error sending request")).toBe("offline");
    expect(sourceProblem("network: timeout")).toBe("offline");
    expect(sourceProblem("network: broken JSON: expected value")).toBe("answer");
    expect(sourceProblem("network: 404 Not Found: x")).toBe("other");
    expect(sourceProblem("network: 429 Too Many Requests: x")).toBe("limit");
    expect(sourceProblem("refused: 401 Unauthorized: x")).toBe("auth");
    expect(sourceProblem("refused: [AUTHENTICATIONFAILED] Invalid credentials")).toBe("auth");
    expect(sourceProblem("limit: 429 Too Many Requests")).toBe("limit");
    expect(sourceProblem("missing: adzuna key")).toBe("missing");
    expect(sourceProblem("network: Himalayas reported 40 of 50 checked offers closed at once - none deleted")).toBe("closed_at_once");
    expect(sourceProblem("database: disk full")).toBe("other");
  });
});
