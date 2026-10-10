import { afterEach, expect, it, vi } from "vitest";
import { createTimelineLabels } from "./timelineLabels";
import { buildLibraryIndex } from "./libraryIndex";
import { createManagedSourceIndex } from "./managedSourceIndex";
import type { ArchiveRecord, CategoryRecord } from "../domain";
import { setLocale } from "./i18n";

afterEach(() => vi.restoreAllMocks());
function archive(id: string, categoryId?: string): ArchiveRecord {
  return { id, name: id, categoryId, category: "", sources: [], sourcePath: "", tags: [], kind: "file", storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createdAt: 0, updatedAt: 0, totalBytes: 0 };
}
for (const size of [100, 500, 1000]) {
  it(`formats ${size} timestamps once each despite nine labels per row`, () => {
    setLocale("zh-CN");
    const original = Intl.DateTimeFormat;
    let formats = 0;
    const constructors = vi.spyOn(Intl, "DateTimeFormat").mockImplementation(function (language, options) {
      const formatter = new original(language, options);
      return { format(value: Date) { formats++; return formatter.format(value); } } as Intl.DateTimeFormat;
    });
    const labels = createTimelineLabels(), now = new Date(2026, 9, 9, 12);
    const yesterday = new Date(2026, 9, 8, 0).getTime();
    for (let index = 0; index < size; index++) for (let usage = 0; usage < 9; usage++) {
      expect(labels(yesterday + index * 60_000, "zh-CN", now)).toContain("10月8日");
    }
    expect(constructors).toHaveBeenCalledTimes(2); expect(formats).toBe(size);
  });
  it(`reuses the category index for ${size} category rows and repeated tree expansion`, () => {
    let parentReads = 0;
    const categories: CategoryRecord[] = Array.from({ length: size }, (_, index) => ({ id: `category-${index}`, name: `Category ${index}`, get parentId() { parentReads++; return null; } }));
    const index = buildLibraryIndex(categories, categories.map(category => archive(category.id, category.id)));
    expect(parentReads).toBe(size * 2);
    for (let expansion = 0; expansion < 3; expansion++) for (let row = 0; row < size; row++) {
      expect(index.counts.get(`category-${row}`)).toBe(1);
      expect(index.descendants(`category-${row}`).size).toBe(1);
    }
    expect(parentReads).toBe(size * 2);
  });
}
it("normalizes 1000 managed roots only during construction while checking 5000 candidates", () => {
  let pathReads = 0;
  const roots = Array.from({ length: 1000 }, (_, i) => {
    const record = archive(String(i));
    record.sources = [{ id: String(i), name: String(i), kind: "folder", get path() { pathReads++; return `C:/Synthetic/${i}`; } }];
    return record;
  });
  const index = createManagedSourceIndex(roots);
  for (let i = 0; i < 5000; i++) expect(index.has({ path: `c:/synthetic/${i % 1000}/slot.sav`, kind: "file" })).toBe(true);
  expect(pathReads).toBe(1000);
});
