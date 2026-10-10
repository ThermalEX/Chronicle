import { invoke } from "@tauri-apps/api/core";
export interface ProtocolSnapshot { id: string; entry_id: string; title: string; note?: string; created_at_ms: number; size_bytes: number; device_id?: string; device_name?: string }
export interface SyncOperation { id: string; action: "upload" | "download" | "uploadRevision" | "downloadRevision" | "recycleLocal" | "recycleRemote" | "conflict" | "unchanged"; selected: boolean; snapshot: ProtocolSnapshot; reason: string | null; deletion: { operationId: string; device: { id: string; name: string; revision: number } } | null }
export interface SourcePlan { sourceId: string; sourceName: string; upgradeRequired: boolean; error: string | null; operations: SyncOperation[]; indexRepairId?: string | null; deviceRecordId?: string | null; archiveNames: Record<string, string>; devices: { id: string; name: string; revision: number }[] }
export interface SyncPlan { id: string; sources: SourcePlan[] }
export interface OperationResult { operationId: string; sourceId: string; status: "success" | "failed" | "cancelled"; error: string | null }
export function executable(op: SyncOperation): boolean { return op.action !== "conflict" && op.action !== "unchanged"; }
export function defaultSelection(ops: SyncOperation[]): string[] { return ops.filter(executable).map((op) => op.id); }
export function defaultPlanSelection(sources: SourcePlan[]): string[] {
  return sources.filter(source => !source.error && !source.upgradeRequired).flatMap(source => [...defaultSelection(source.operations), ...[source.indexRepairId, source.deviceRecordId].filter((id): id is string => Boolean(id))]);
}
export function quickSyncPlan(plan: SyncPlan, showDetails: boolean): { requiresReview: boolean; operationIds: string[] } {
  const available = plan.sources.filter((source) => !source.error && !source.upgradeRequired);
  return {
    requiresReview: showDetails || plan.sources.some((source) => source.upgradeRequired)
      || available.some((source) => source.operations.some((op) => ["recycleLocal", "recycleRemote", "conflict"].includes(op.action))),
    operationIds: defaultPlanSelection(available),
  };
}
export const snapshotSync = {
  preview: (requestId: string, sourceIds: string[], entryIds: string[], snapshotId?: string) => invoke<SyncPlan>("preview_snapshot_sync", { requestId, sourceIds, entryIds, snapshotId: snapshotId ?? null }),
  apply: (planId: string, operationIds: string[]) => invoke<OperationResult[]>("apply_snapshot_sync_plan", { planId, operationIds }),
  cancel: (requestId: string) => invoke<void>("cancel_snapshot_sync", { requestId }),
  enable: (sourceId: string) => invoke<void>("enable_snapshot_sync_protocol", { sourceId, allDevicesUpgraded: true }),
};
