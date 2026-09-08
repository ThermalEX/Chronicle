import { describe, expect, it } from "vitest";
import { githubClassicPatUrl, githubRepositoryName } from "./githubPat";

describe("GitHub classic PAT link", () => {
  it("pre-fills the Chronicle label and repository scope", () => {
    expect(githubClassicPatUrl()).toBe(
      "https://github.com/settings/tokens/new?description=Chronicle+%E4%BA%91%E7%AB%AF%E5%90%8C%E6%AD%A5&scopes=repo",
    );
  });

  it("derives a stable unique default repository name from the source id", () => {
    expect(githubRepositoryName("a1b2c3d4-e5f6-7890-abcd-ef0123456789"))
      .toBe("chronicle-library-a1b2c3d4");
  });
});
