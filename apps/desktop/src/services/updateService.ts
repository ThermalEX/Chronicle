import { invoke, isTauri } from "@tauri-apps/api/core";
import type { UpdateChannel } from "./settings";

const RELEASE_FEED_URL = "https://github.com/ThermalEX/Chronicle/releases.atom";
const RELEASE_DOWNLOAD_BASE_URL = "https://github.com/ThermalEX/Chronicle/releases/download";

export interface GithubReleaseAsset {
  name: string;
  browser_download_url: string;
  size: number;
}

export interface GithubRelease {
  tag_name: string;
  name: string;
  body: string;
  html_url: string;
  prerelease: boolean;
  draft: boolean;
  published_at: string;
  assets: GithubReleaseAsset[];
}

export interface ReleaseAsset {
  name: string;
  browserDownloadUrl: string;
  size: number;
}

export interface ReleaseUpdate {
  version: string;
  title: string;
  notes: string;
  releaseUrl: string;
  downloadUrl: string;
  publishedAt: string;
  installer?: ReleaseAsset;
  checksumUrl?: string;
}

interface ParsedVersion {
  major: number;
  minor: number;
  patch: number;
  prerelease?: string;
}

function parseVersion(value: string): ParsedVersion | undefined {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(value.trim());
  if (!match) return undefined;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4],
  };
}

function comparePrerelease(left: string, right: string): number {
  const leftParts = left.split(".");
  const rightParts = right.split(".");
  const maxLength = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < maxLength; index += 1) {
    const a = leftParts[index];
    const b = rightParts[index];
    if (a === b) continue;
    if (a === undefined) return -1;
    if (b === undefined) return 1;
    const aNumber = /^\d+$/.test(a);
    const bNumber = /^\d+$/.test(b);
    if (aNumber && bNumber) return Number(a) - Number(b);
    if (aNumber) return -1;
    if (bNumber) return 1;
    return a.localeCompare(b);
  }
  return 0;
}

export function compareVersions(left: string, right: string): number | undefined {
  const a = parseVersion(left);
  const b = parseVersion(right);
  if (!a || !b) return undefined;
  for (const key of ["major", "minor", "patch"] as const) {
    if (a[key] !== b[key]) return a[key] - b[key];
  }
  if (!a.prerelease && !b.prerelease) return 0;
  if (!a.prerelease) return 1;
  if (!b.prerelease) return -1;
  return comparePrerelease(a.prerelease, b.prerelease);
}

function releaseAsset(asset: GithubReleaseAsset): ReleaseAsset {
  return {
    name: asset.name,
    browserDownloadUrl: asset.browser_download_url,
    size: asset.size,
  };
}

function isWindowsInstaller(name: string): boolean {
  return /^Chronicle_.+_x64-setup\.exe$/i.test(name);
}

function decodeXml(value: string): string {
  return value.replace(/&#(x[0-9a-f]+|\d+);|&(amp|lt|gt|quot|apos);/gi, (entity, numeric) => {
    if (numeric) {
      const codePoint = numeric.toLowerCase().startsWith("x")
        ? Number.parseInt(numeric.slice(1), 16)
        : Number.parseInt(numeric, 10);
      return String.fromCodePoint(codePoint);
    }
    return ({ amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" } as Record<string, string>)[entity.slice(1, -1).toLowerCase()] ?? entity;
  });
}

function atomTag(entry: string, name: string): string {
  const match = new RegExp(`<${name}(?:\\s[^>]*)?>([\\s\\S]*?)</${name}>`, "i").exec(entry);
  return decodeXml(match?.[1]?.trim() ?? "");
}

function atomReleaseUrl(entry: string): string | undefined {
  const link = /<link\b(?=[^>]*\brel=["']alternate["'])[^>]*\bhref=["']([^"']+)["'][^>]*\/?\s*>/i.exec(entry);
  return link ? decodeXml(link[1]) : undefined;
}

function releaseNotesFromHtml(html: string): string {
  return html
    .replace(/<h2[^>]*>/gi, "## ")
    .replace(/<\/h2>/gi, "\n\n")
    .replace(/<li[^>]*>/gi, "- ")
    .replace(/<\/li>/gi, "\n")
    .replace(/<br\s*\/?\s*>/gi, "\n")
    .replace(/<\/p>/gi, "\n\n")
    .replace(/<[^>]+>/g, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function releaseAssets(tag: string): GithubReleaseAsset[] {
  const version = tag.replace(/^v/, "");
  const installerName = `Chronicle_${version}_x64-setup.exe`;
  const releaseBaseUrl = `${RELEASE_DOWNLOAD_BASE_URL}/${encodeURIComponent(tag)}`;
  return [
    { name: installerName, browser_download_url: `${releaseBaseUrl}/${installerName}`, size: 0 },
    { name: "SHA256SUMS.txt", browser_download_url: `${releaseBaseUrl}/SHA256SUMS.txt`, size: 0 },
  ];
}

export function releasesFromAtom(feed: string): GithubRelease[] {
  return [...feed.matchAll(/<entry(?:\s[^>]*)?>([\s\S]*?)<\/entry>/gi)].flatMap((match) => {
    const entry = match[1];
    const htmlUrl = atomReleaseUrl(entry);
    const tag = htmlUrl?.match(/\/releases\/tag\/([^/?#]+)$/)?.[1];
    if (!tag) return [];
    const content = atomTag(entry, "content");
    return [{
      tag_name: tag,
      name: atomTag(entry, "title") || `Chronicle ${tag}`,
      body: releaseNotesFromHtml(content),
      html_url: htmlUrl,
      prerelease: /^v?\d+\.\d+\.\d+-/.test(tag),
      draft: false,
      published_at: atomTag(entry, "updated"),
      assets: releaseAssets(tag),
    }];
  });
}

export function selectUpdate(
  releases: GithubRelease[],
  currentVersion: string,
  channel: UpdateChannel,
): ReleaseUpdate | undefined {
  const eligible = releases.filter((release) => {
    const comparison = compareVersions(release.tag_name, currentVersion);
    return !release.draft
      && (channel === "beta" || !release.prerelease)
      && comparison !== undefined
      && comparison > 0;
  });
  const newest = eligible.reduce<GithubRelease | undefined>((selected, release) => {
    if (!selected) return release;
    return (compareVersions(release.tag_name, selected.tag_name) ?? -1) > 0 ? release : selected;
  }, undefined);
  if (!newest) return undefined;

  const installer = newest.assets.find((asset) => isWindowsInstaller(asset.name));
  const checksum = newest.assets.find((asset) => asset.name === "SHA256SUMS.txt");
  return {
    version: newest.tag_name.replace(/^v/, ""),
    title: newest.name || `Chronicle ${newest.tag_name}`,
    notes: newest.body || "此版本未提供更新说明。",
    releaseUrl: newest.html_url,
    downloadUrl: installer?.browser_download_url ?? newest.html_url,
    publishedAt: newest.published_at,
    ...(installer ? { installer: releaseAsset(installer) } : {}),
    ...(checksum ? { checksumUrl: checksum.browser_download_url } : {}),
  };
}

export async function checkForUpdate(
  channel: UpdateChannel,
  currentVersion: string,
  request: typeof fetch = fetch,
): Promise<ReleaseUpdate | undefined> {
  const feed = isTauri()
    ? await invoke<string>("fetch_release_feed")
    : await (async () => {
      const response = await request(RELEASE_FEED_URL, { headers: { Accept: "application/atom+xml" } });
      if (!response.ok) throw new Error(`更新检查失败（HTTP ${response.status}）`);
      return response.text();
    })();
  return selectUpdate(releasesFromAtom(feed), currentVersion, channel);
}
