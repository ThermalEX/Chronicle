import { t } from "./i18n";
export function normalizeRegistryPath(input: string): string {
  const parts = input.trim().replaceAll("/", "\\").split("\\");
  const aliases: Record<string, string> = { HKCU: "HKEY_CURRENT_USER", HKLM: "HKEY_LOCAL_MACHINE" };
  const hive = parts[0].toUpperCase();
  parts[0] = aliases[hive] ?? hive;
  if (!Object.values(aliases).includes(parts[0]) || parts.length < 2 || parts.some((part) => !part || part === "." || part === ".." || part.includes("\0"))) {
    throw new Error(t("请输入完整注册表子键路径，例如 HKCU\\Software\\Example；不能选择整个根键。"));
  }
  return parts.join("\\");
}

export function exclusionMatcher(patterns: string[] = []): (path: string, directory?: boolean) => boolean {
  const rules = patterns.filter((value) => value.trim()).map((value) => {
    const pattern = value.trim().replaceAll("\\", "/");
    if (pattern.startsWith("/") || pattern.includes(":") || pattern.includes("\0") || pattern.startsWith("!") || pattern.split("/").some((part) => part === "." || part === "..")) throw new Error(t("排除规则必须是来源内的相对路径"));
    if (/[\[\]{}]/.test(pattern)) throw new Error(t("浏览器预览版排除规则仅支持 *、? 和 **；复杂模式请在桌面版使用"));
    const directory = pattern.endsWith("/");
    const body = directory ? pattern.slice(0, -1) : pattern;
    const expression = body.split(/(\*\*\/|\*\*|\*|\?)/).map((part) => part === "**/" ? "(?:[^/]+/)*" : part === "**" ? ".*" : part === "*" ? "[^/]*" : part === "?" ? "[^/]" : part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("");
    return { directory, regex: new RegExp(`${body.includes("/") ? "^" : "(?:^|/)"}${expression}$`, "i") };
  });
  return (path, directory = false) => {
    const parts = path.replaceAll("\\", "/").split("/");
    return rules.some((rule) => {
      if (rule.regex.test(parts.join("/")) && (!rule.directory || directory)) return true;
      return parts.slice(0, -1).some((_, index) => rule.regex.test(parts.slice(0, index + 1).join("/")));
    });
  };
}
