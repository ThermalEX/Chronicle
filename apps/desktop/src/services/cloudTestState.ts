import { sourceTestKey } from "./opendal";
import type { CloudSource } from "./settings";

const successfulTests = new Map<string, string>();

export function recordCloudSourceTest(source: CloudSource, secrets: Record<string, string> = {}): void {
  successfulTests.set(source.id, sourceTestKey(source, secrets));
}

export function hasCloudSourceTestPassed(source: CloudSource, secrets: Record<string, string> = {}): boolean {
  return successfulTests.get(source.id) === sourceTestKey(source, secrets);
}
