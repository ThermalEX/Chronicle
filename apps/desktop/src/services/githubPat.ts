export function githubClassicPatUrl(): string {
  const query = new URLSearchParams({
    description: "Chronicle 云端同步",
    scopes: "repo",
  });
  return `https://github.com/settings/tokens/new?${query.toString()}`;
}
