import { describe, expect, it } from "vitest";
import { stoppedConnect } from "./syncConnect";

describe("stoppedConnect", () => {
  it("a login approved just before back is kept - the last look comes before the status", async () => {
    let approved = false;
    const poll = async () => {
      approved = true;
      return true;
    };
    const status = async () => ({ connected: approved });
    expect(await stoppedConnect(poll, status)).toBe("sync.connected");
  });

  it("no login - connecting stopped", async () => {
    expect(await stoppedConnect(async () => false, async () => ({ connected: false }))).toBe("sync.connectStopped");
  });

  it("a failed last look does not hide a connection made meanwhile", async () => {
    const poll = () => Promise.reject(new Error("offline"));
    expect(await stoppedConnect(poll, async () => ({ connected: true }))).toBe("sync.connected");
  });

  it("a failed status counts as not connected, without throwing", async () => {
    const status = () => Promise.reject(new Error("offline"));
    expect(await stoppedConnect(async () => true, status)).toBe("sync.connectStopped");
  });
});
