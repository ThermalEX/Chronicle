import { beforeEach, describe, expect, it, vi } from "vitest";

const { isTauri, openUrl } = vi.hoisted(() => ({
  isTauri: vi.fn(),
  openUrl: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ isTauri }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));

import { openExternalUrl } from "./externalBrowser";

describe("openExternalUrl", () => {
  beforeEach(() => {
    isTauri.mockReset();
    openUrl.mockReset();
  });

  it("uses the desktop opener so a GitHub URL goes to the default browser", async () => {
    isTauri.mockReturnValue(true);
    openUrl.mockResolvedValue(undefined);

    await openExternalUrl("https://github.com/settings/tokens/new");

    expect(openUrl).toHaveBeenCalledWith("https://github.com/settings/tokens/new");
  });
});
