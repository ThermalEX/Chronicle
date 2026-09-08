import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ArchiveSyncMode } from "../domain";
import type { CloudSource } from "./settings";

export type RemoteItem = {
  id: string;
  name: string;
  kind: "config" | "archive";
  protected: boolean;
  snapshotCount: number;
  sizeBytes: number;
  updatedAt?: number;
  syncMode: ArchiveSyncMode;
};

export type CloudPreview = { sourceName: string; libraryId?: string; items: RemoteItem[] };
export type CloudSyncResult = { status: "uploaded" | "downloaded" | "current" | "conflict"; message: string };
export type CreatedGitHubRepository = { repository: string; branch: string };

function desktopOnly(): never { throw new Error("云同步仅在 Chronicle 桌面端可用"); }

export const cloudRepository = {
  saveCredential(source: CloudSource, password: string): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("save_cloud_credential", { sourceId: source.id, credentialRef: source.credentialRef, password, provider: source.provider });
  },
  createGitHubRepository(source: CloudSource, repositoryName: string, password: string): Promise<CreatedGitHubRepository> {
    if (!isTauri()) desktopOnly();
    return invoke("create_github_repository", { source, repositoryName, password });
  },
  test(source: CloudSource, password: string): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("test_cloud_source", { source, password });
  },
  preview(sourceId: string): Promise<CloudPreview> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_preview", { sourceId });
  },
  sync(sourceId: string, entryId: string): Promise<CloudSyncResult> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_sync_entry", { sourceId, entryId });
  },
  upload(sourceId: string, entryId: string): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_overwrite_upload", { sourceId, entryId });
  },
  download(sourceId: string, entryId: string): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_overwrite_download", { sourceId, entryId });
  },
  delete(sourceId: string, entryIds: string[]): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_delete_entries", { sourceId, entryIds });
  },
  setSyncMode(sourceId: string, entryId: string, syncMode: ArchiveSyncMode): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_set_entry_sync_mode", { sourceId, entryId, syncMode });
  },
};
