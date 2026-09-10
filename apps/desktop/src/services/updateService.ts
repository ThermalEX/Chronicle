import type { UpdateChannel } from "./settings";

const RELEASES_URL = "https://api.github.com/repos/ThermalEX/Chronicle/releases?per_page=20";

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
  const response = await request(RELEASES_URL, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) throw new Error(`更新检查失败（HTTP ${response.status}）`);
  const releases = await response.json() as GithubRelease[];
  return selectUpdate(releases, currentVersion, channel);
}
