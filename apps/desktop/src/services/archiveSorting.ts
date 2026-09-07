export type ArchiveSortMode = "newest" | "oldest" | "nameAsc" | "nameDesc";

const pinyinCollator = new Intl.Collator("zh-CN-u-co-pinyin", {
  numeric: true,
  sensitivity: "base",
});

function nameGroup(name: string): number {
  const first = name.trim().charAt(0);
  if (/^\p{Nd}$/u.test(first)) return 0;
  if (/^[A-Za-z]$/.test(first)) return 1;
  if (/^\p{Script=Han}$/u.test(first)) return 2;
  return 3;
}

export function compareArchiveNames(left: string, right: string, descending = false): number {
  const groupDifference = nameGroup(left) - nameGroup(right);
  if (groupDifference) return groupDifference;
  const comparison = pinyinCollator.compare(left, right);
  return descending ? -comparison : comparison;
}
