import { describe, expect, it } from "vitest";
import { exclusionMatcher, normalizeRegistryPath } from "./backupRules";

describe("backup source rules", () => {
  it("matches relative glob files and complete directory subtrees", () => {
    const excluded = exclusionMatcher(["*.tmp", "cache/", "root/*.log"]);
    expect(excluded("nested/save.tmp")).toBe(true);
    expect(excluded("nested/SAVE.TMP")).toBe(true);
    expect(excluded("nested/cache/data.bin")).toBe(true);
    expect(excluded("cache", true)).toBe(true);
    expect(excluded("cacheable/data.bin")).toBe(false);
    expect(excluded("root/debug.log")).toBe(true);
    expect(excluded("nested/root/debug.log")).toBe(false);
    expect(excluded("save.dat")).toBe(false);
  });
  it("allows globstar to match zero or more nested directories", () => {
    const excluded = exclusionMatcher(["state/**/cache/"]);
    expect(excluded("state/cache/file")).toBe(true);
    expect(excluded("state/a/b/cache/file")).toBe(true);
    expect(excluded("other/state/cache/file")).toBe(false);
  });
  it("rejects patterns outside the source", () => {
    for (const pattern of ["../save", "C:/save", "/save"]) expect(() => exclusionMatcher([pattern])).toThrow();
  });
  it("normalizes registry aliases and refuses entire hives", () => {
    expect(normalizeRegistryPath(" HKCU\\Software\\Example ")).toBe("HKEY_CURRENT_USER\\Software\\Example");
    expect(normalizeRegistryPath("hklm/Software/Example")).toBe("HKEY_LOCAL_MACHINE\\Software\\Example");
    expect(() => normalizeRegistryPath("HKCU")).toThrow();
    expect(() => normalizeRegistryPath("HKCU\\..\\Example")).toThrow();
    expect(() => normalizeRegistryPath("HKCR\\Example")).toThrow();
  });
});
