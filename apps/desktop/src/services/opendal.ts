import type { CloudSource } from "./settings";

export const publicConfigKeys = ["endpoint", "bucket", "bucket_id", "region", "container", "owner", "repo", "username", "account_name"];
export const openDalTemplates = [
  { scheme: "s3", label: "S3 / 兼容 S3 服务", fields: ["bucket", "region", "endpoint"], secrets: ["access_key_id", "secret_access_key"] },
  { scheme: "b2", label: "Backblaze B2", fields: ["bucket", "bucket_id"], secrets: ["application_key_id", "application_key"] },
  { scheme: "azblob", label: "Azure Blob Storage", fields: ["container", "endpoint", "account_name"], secrets: ["account_key"] },
  { scheme: "gcs", label: "Google Cloud Storage", fields: ["bucket"], secrets: ["credential"] },
  { scheme: "oss", label: "阿里云 OSS", fields: ["bucket", "endpoint"], secrets: ["access_key_id", "access_key_secret"] },
  { scheme: "cos", label: "腾讯云 COS", fields: ["bucket", "endpoint"], secrets: ["secret_id", "secret_key"] },
  { scheme: "obs", label: "华为云 OBS", fields: ["bucket", "endpoint"], secrets: ["access_key_id", "secret_access_key"] },
  { scheme: "tos", label: "火山引擎 TOS", fields: ["bucket", "endpoint", "region"], secrets: ["access_key_id", "secret_access_key"] },
  { scheme: "swift", label: "OpenStack Swift", fields: ["container", "endpoint"], secrets: ["token"] },
  { scheme: "github", label: "GitHub（OpenDAL，默认分支）", fields: ["owner", "repo"], secrets: ["token"] },
  { scheme: "webdav", label: "WebDAV（OpenDAL）", fields: ["endpoint", "username"], secrets: ["password"] },
] as const;

export function configureOpenDal(source: CloudSource, scheme: string): void {
  const template = openDalTemplates.find((item) => item.scheme === scheme);
  if (!template) throw new Error("此 OpenDAL 服务未编译");
  source.scheme = scheme;
  source.config = Object.fromEntries(template.fields.map((key) => [key, ""]));
  source.secretKeys = [...template.secrets];
}

export function validateAdvancedKey(key: string): string {
  if (!/^[a-z][a-z0-9_]*$/.test(key)) return "字段名须使用小写英文、数字和下划线";
  if (["root", "branch", "credential_path", "google_application_credentials", "aws_profile"].includes(key) || key.endsWith("_path") || key.endsWith("_file")) return "不允许覆盖根目录、分支或读取本地凭据文件";
  return "";
}

export function sourceTestKey(source: CloudSource, secrets: Record<string, string>): string {
  // Ephemeral, memory-only comparison. Never store this value in settings or logs.
  const { syncEnabled: _syncEnabled, ...connection } = source;
  return JSON.stringify([connection, secrets]);
}

export function secretPatch(secrets: Record<string, string>): Record<string, string> {
  return Object.fromEntries(Object.entries(secrets).filter(([, value]) => value !== ""));
}

export function validateOpenDal(source: CloudSource): string {
  if (!openDalTemplates.some((item) => item.scheme === source.scheme)) return "请选择已编译的 OpenDAL 服务";
  if (!source.name.trim()) return "请填写同步源名称";
  if (source.remotePath.includes("\\") || source.remotePath.split("/").some((part) => part === ".." || part === ".")) return "远端目录不能包含父路径或反斜杠";
  for (const [key, value] of Object.entries(source.config ?? {})) {
    if (!publicConfigKeys.includes(key) || source.secretKeys?.includes(key)) return `字段 ${key} 必须作为机密配置保存`;
    if (!value.trim() && key !== "endpoint" && key !== "region" && key !== "username" && key !== "account_name") return `请填写 ${key}`;
    if (key === "endpoint" && value.trim()) {
      try {
        const endpoint = new URL(value);
        if (!["https:", "http:"].includes(endpoint.protocol) || endpoint.username || endpoint.password || endpoint.search || endpoint.hash) {
          return "endpoint 须为 HTTP(S) 地址，不能包含用户名、密码、查询参数或片段";
        }
      } catch { return "endpoint 须为有效的 HTTP(S) 地址"; }
    }
  }
  for (const key of source.secretKeys ?? []) {
    const error = validateAdvancedKey(key);
    if (error) return `${key}：${error}`;
  }
  return "";
}
