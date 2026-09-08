export function githubClassicPatUrl(): string {
  const query = new URLSearchParams({
    description: "Chronicle 云端同步",
    scopes: "repo",
  });
  return `https://github.com/settings/tokens/new?${query.toString()}`;
}

export function githubRepositoryName(sourceId: string): string {
  const suffix = sourceId.replace(/[^a-zA-Z0-9]/g, "").slice(0, 8).toLocaleLowerCase();
  return `chronicle-library-${suffix || "backup"}`;
}
