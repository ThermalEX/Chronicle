import type { CloudProvider, CloudSource } from "./settings";

export function toggleSourceSync(source: CloudSource): CloudSource {
  return { ...source, syncEnabled: !source.syncEnabled };
}

export function newCloudSource(provider: CloudProvider, id: string, index: number): CloudSource {
  const common = {
    id,
    endpoint: "",
    username: "",
    remotePath: "/Chronicle",
    syncEnabled: false,
  };
  if (provider === "legacy_github") return { ...common, name: `GitHub ${index}`, provider, credentialRef: `chronicle-github:${id}`, repository: "", branch: "main" };
  if (provider === "opendal") return { ...common, name: "OpenDAL", provider, credentialRef: `chronicle-opendal:${id}` };
  return { ...common, name: `WebDAV ${index}`, provider, credentialRef: `chronicle-webdav:${id}` };
}
