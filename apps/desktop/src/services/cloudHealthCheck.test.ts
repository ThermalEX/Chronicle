import { describe, expect, it } from "vitest";
import { runCloudHealthCheck } from "./cloudHealthCheck";

const sources = [
  { id: "r2", name: "Cloudflare R2" },
  { id: "b2", name: "Backblaze B2" },
] as const;

function deferred() {
  let resolve!: () => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<void>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

describe("cloud health check", () => {
  it("starts every source together and retains each independent result", async () => {
    const r2 = deferred();
    const b2 = deferred();
    const calls: string[] = [];
    const updates: Array<Array<{ id: string; status: string; reason?: string }>> = [];
    const run = runCloudHealthCheck(sources, async (source) => {
      calls.push(source.id);
      return source.id === "r2" ? r2.promise : b2.promise;
    }, (items) => updates.push(items));

    await Promise.resolve();
    expect(calls).toEqual(["r2", "b2"]);
    expect(updates[0]).toEqual([
      { id: "r2", name: "Cloudflare R2", status: "checking" },
      { id: "b2", name: "Backblaze B2", status: "checking" },
    ]);

    r2.resolve();
    b2.reject(new Error("Access denied"));
    await expect(run).resolves.toEqual([
      { id: "r2", name: "Cloudflare R2", status: "passed" },
      { id: "b2", name: "Backblaze B2", status: "failed", reason: "Access denied" },
    ]);
  });
});
