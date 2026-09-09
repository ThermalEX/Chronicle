import { describe, expect, it } from "vitest";
import { newCloudSource, toggleSourceSync } from "./cloudSourceControls";

describe("cloud source controls", () => {
  it("creates new sources paused and toggles only the chosen source", () => {
    const source = newCloudSource("legacy_webdav", "source-a", 1);
    expect(source.syncEnabled).toBe(false);
    expect(toggleSourceSync(source)).toMatchObject({ id: "source-a", syncEnabled: true });
    expect(source.syncEnabled).toBe(false);
  });
});
