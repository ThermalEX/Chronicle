import { describe, expect, it } from "vitest";
import { runAcrossEnabledSources } from "./multiSourceSync";
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
});
