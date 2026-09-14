import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ArchiveSyncMode } from "../domain";
import { saveCloudSettings, type CloudSettings, type CloudSource } from "./settings";
import { runAcrossEnabledSources, runItemsBySource } from "./multiSourceSync";

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
export type CloudSourceStatus = { sourceId: string; credentialSaved: boolean };

function desktopOnly(): never { throw new Error("云同步仅在 Chronicle 桌面端可用"); }

export const cloudRepository = {
  sourceStatuses(): Promise<CloudSourceStatus[]> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_source_statuses");
  },
  saveOpenDalCredential(source: CloudSource, secrets: Record<string, string>): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("save_opendal_credential", { source, secrets });
  },
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
  download(sourceId: string, entryId: string, useCloudCategoryTree = false): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_overwrite_download", { sourceId, entryId, useCloudCategoryTree });
  },
  uploadApplicationSettings(sourceId: string): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_upload_application_settings", { sourceId });
  },
  uploadSyncMetadata(sourceId: string, entryIds: string[]): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_upload_sync_metadata", { sourceId, entryIds });
  },
  uploadEntryCategoryTree(sourceId: string, entryId: string): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_upload_entry_category_tree", { sourceId, entryId });
  },
  uploadEntryCategoryTrees(sourceId: string, entryIds: string[]): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_upload_entry_category_trees", { sourceId, entryIds });
  },
  downloadApplicationSettings(sourceId: string, apply = false): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_download_application_settings", { sourceId, apply });
  },
  delete(sourceId: string, entryIds: string[]): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_delete_entries", { sourceId, entryIds });
  },
  deleteConfigurations(sourceId: string, configurationIds: string[]): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_delete_configurations", { sourceId, configurationIds });
  },
  setSyncMode(sourceId: string, entryId: string, syncMode: ArchiveSyncMode): Promise<void> {
    if (!isTauri()) desktopOnly();
    return invoke("cloud_set_entry_sync_mode", { sourceId, entryId, syncMode });
  },
};

/** Upload the archive payload and only that archive's category ancestry. */
export async function uploadArchiveWithCategoryTree(sourceId: string, entryId: string): Promise<void> {
  await cloudRepository.upload(sourceId, entryId);
  await cloudRepository.uploadEntryCategoryTree(sourceId, entryId);
}

/** Each source completes its own archive queue and metadata independently of slower sources. */
export async function syncArchivesAcrossSources<T extends { id: string }>(sources: CloudSource[], archives: T[], onSettled?: () => void) {
  const perSource = await Promise.all(sources.map(async (source) => {
    const outcomes = await runItemsBySource([source], archives, (target, archive) => cloudRepository.sync(target.id, archive.id), onSettled);
    const entryIds = outcomes.filter((outcome) => outcome.status === "fulfilled" && outcome.value.status !== "conflict").map((outcome) => outcome.item.id);
    const metadataOutcomes = await runAcrossEnabledSources([source], async () => {
      if (entryIds.length) await cloudRepository.uploadSyncMetadata(source.id, entryIds);
    }, onSettled);
    return { outcomes, metadataOutcomes };
  }));
  const outcomes = perSource.flatMap((result) => result.outcomes);
  const metadataOutcomes = perSource.flatMap((result) => result.metadataOutcomes);
  return { outcomes, metadataOutcomes, hasFailures: [...outcomes, ...metadataOutcomes].some((outcome) => outcome.status === "rejected") };
}

/** Do not enable sources until every credential write has succeeded. */
export async function saveCloudConfiguration(settings: CloudSettings, credentials: {
  source: CloudSource;
  password?: string;
  secrets?: Record<string, string>;
}[]): Promise<void> {
  for (const value of credentials) {
    if (value.source.provider === "opendal") await cloudRepository.saveOpenDalCredential(value.source, value.secrets ?? {});
    else if (value.password) await cloudRepository.saveCredential(value.source, value.password);
  }
  await saveCloudSettings(settings);
}
