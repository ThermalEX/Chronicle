import { describe, expect, it } from "vitest";
import { runAcrossEnabledSources, runItemsBySource } from "./multiSourceSync";
import type { CloudSource } from "./settings";

const sources: CloudSource[] = [
  { id: "source-a", name: "A", provider: "legacy_webdav", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "a", syncEnabled: true },
  { id: "source-b", name: "B", provider: "legacy_webdav", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "b", syncEnabled: true },
];

describe("runAcrossEnabledSources", () => {
  it("starts all enabled sources together and preserves an independent failure", async () => {
    const started: string[] = [];
    let releaseA!: () => void;
    let rejectB!: (reason: Error) => void;
    const waitA = new Promise<void>((resolve) => { releaseA = resolve; });
    const waitB = new Promise<void>((_, reject) => { rejectB = reject; });

    const run = runAcrossEnabledSources(sources, (source) => {
      started.push(source.id);
      return source.id === "source-a" ? waitA : waitB;
    });

    await Promise.resolve();
    expect(started).toEqual(["source-a", "source-b"]);
    releaseA();
    rejectB(new Error("permission denied"));

    const outcomes = await run;
    expect(outcomes.map((outcome) => outcome.status)).toEqual(["fulfilled", "rejected"]);
    expect(outcomes[1].status).toBe("rejected");
    if (outcomes[1].status === "rejected") expect(outcomes[1].reason).toBeInstanceOf(Error);
  });

  it("reports progress once for every settled source", async () => {
    let settled = 0;
    await runAcrossEnabledSources(sources, async () => undefined, () => { settled += 1; });
    expect(settled).toBe(2);
  });

  it("runs sources in parallel while keeping each source's items serial", async () => {
    const started: string[] = [];
    const active = new Map<string, number>();
    const peak = new Map<string, number>();

    const outcomes = await runItemsBySource(sources, ["one", "two"], async (source, item) => {
      started.push(`${source.id}:${item}`);
      const current = (active.get(source.id) ?? 0) + 1;
      active.set(source.id, current);
      peak.set(source.id, Math.max(peak.get(source.id) ?? 0, current));
      await Promise.resolve();
      active.set(source.id, current - 1);
      return item;
    });

    expect(started.slice(0, 2)).toEqual(["source-a:one", "source-b:one"]);
    expect(peak.get("source-a")).toBe(1);
    expect(peak.get("source-b")).toBe(1);
    expect(outcomes).toHaveLength(4);
  });
});
