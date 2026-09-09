<script setup lang="ts">
import {
  Check, ChevronLeft, ChevronRight, Clock3, CloudCog, File, Info,
  Folder, FolderArchive, FolderOpen, HardDrive, LockKeyhole, MoreHorizontal, Moon, Pencil, Plus, RotateCcw, Save,
  Search, Settings2, SlidersHorizontal, UploadCloud, X,
  Sun, Trash2,
} from "@lucide/vue";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import CloudSettingsDialog from "./components/CloudCenterDialog.vue";
import CloudHealthDialog from "./components/CloudHealthDialog.vue";
import ArchiveMetadata from "./components/ArchiveMetadata.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import CreateCategoryDialog from "./components/CreateCategoryDialog.vue";
import CreateArchiveDialog from "./components/CreateArchiveDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import ThemedSelect, { type ThemedSelectOption } from "./components/ThemedSelect.vue";
import type { ArchiveRecord, ArchiveSource, CategoryRecord, CreateArchiveInput, RepositoryInfo, SnapshotProgress, SnapshotRecord, SourceKind } from "./domain";
import { archiveRepository, isTauriRuntime } from "./services/repository";
import { cloudRepository, type CloudSyncResult } from "./services/cloud";
import { runCloudHealthCheck, type CloudHealthCheckItem } from "./services/cloudHealthCheck";
import { diagnosticsRepository } from "./services/diagnostics";
import { diagnosticFromError, type DiagnosticContext } from "./services/diagnosticsCore";
import { compareArchiveNames, type ArchiveSortMode } from "./services/archiveSorting";
import { selectArchivePanelCategory, selectCategoryPanel } from "./services/archiveWorkspace";
import { applyAppearance, normalizeAppearance } from "./services/appearance";
import { floatingMenuStyle, positionFloatingMenu } from "./services/floatingMenu";
import { filterTimeline, type TimelineSort } from "./services/snapshotTimeline";
import { appSettings, cloudLibraryIndicator, cloudSettings, enabledCloudSources, initializeSettings, saveAppSettings, shortcutMatches, type CloudHealth } from "./services/settings";
import { runAcrossEnabledSources, type SourceSyncOutcome } from "./services/multiSourceSync";
import { categoryBreadcrumb } from "./services/categoryBreadcrumb";
import { formatCurrentTime, millisecondsUntilNextMinute } from "./services/currentTime";
import { snapshotButtonProgress } from "./services/snapshotButtonProgress";
import { recordCloudSourceTest } from "./services/cloudTestState";

type CategoryTreeNode = CategoryRecord & {
  nodeType: "category";
  depth: number;
  count: number;
  icon: typeof Folder;
  hasChildren: boolean;
};

type ArchiveTreeNode = {
  nodeType: "archive";
  id: string;
  depth: number;
  archive: ArchiveRecord;
};

const archives = ref<ArchiveRecord[]>([]);
const categoryRecords = ref<CategoryRecord[]>([]);
const snapshots = ref<SnapshotRecord[]>([]);
const selectedCategoryId = ref("all");
const selectedArchiveId = ref<string>();
const selectedSnapshotId = ref<string>();
const activityPanelOpen = ref(false);
const archivePanelCollapsed = ref(false);
const snapshotDescription = ref("");
const snapshotNote = ref("");
const snapshotSearch = ref("");
const snapshotSort = ref<TimelineSort>("newest");
const timelineSortOptions: ThemedSelectOption[] = [
  { value: "newest", label: "时间 新–旧" },
  { value: "oldest", label: "时间 旧–新" },
];
const searchTerm = ref("");
const searchInput = ref<HTMLInputElement>();
const loading = ref(true);
const createDialogOpen = ref(false);
const pendingSources = ref<ArchiveSource[]>([]);
const pickingSource = ref<SourceKind>();
const creatingArchive = ref(false);
const createArchiveError = ref<string>();
const sortMenuOpen = ref(false);
const sortMode = ref<ArchiveSortMode>("newest");
const draggedArchiveId = ref<string>();
const draggedCategoryId = ref<string>();
const categoryDropTarget = ref<string>();
const expandedCategoryIds = ref(new Set<string>());
const activeTreeNodeId = ref("category:all");
const categoryDialogOpen = ref(false);
const creatingCategory = ref(false);
const createCategoryError = ref<string>();
const settingsOpen = ref(false);
const cloudSettingsOpen = ref(false);
const syncingArchive = ref(false);
const snapshotProgress = ref<SnapshotProgress>();
const busyAction = ref<"snapshot" | "restore">();
const savingTags = ref(false);
const savingSnapshotNote = ref(false);
const repositoryInfo = ref<RepositoryInfo>({ path: "", totalBytes: 0 });
const editingArchive = ref<ArchiveRecord>();
const archiveMenuOpen = ref(false);
const trashDropActive = ref(false);
const treeMenu = ref<{ kind: "archive" | "category"; id: string }>();
const treeMenuAnchor = ref<DOMRect>();
const treeMenuStyle = computed(() => treeMenuAnchor.value
  ? floatingMenuStyle(positionFloatingMenu(treeMenuAnchor.value, { width: 168, height: 96 }, { width: window.innerWidth, height: window.innerHeight }))
  : undefined);
const confirmRequest = ref<{ title: string; message: string; confirmLabel: string; destructive: boolean }>();
let confirmResolver: ((confirmed: boolean) => void) | undefined;
const notice = ref<{ type: "success" | "error" | "info"; message: string }>();
let noticeTimer: number | undefined;
const automaticSyncTimers = new Map<string, number>();
const cloudHealth = ref<CloudHealth>({ status: "unchecked" });
const cloudHealthDialogOpen = ref(false);
const cloudHealthCheckRunning = ref(false);
const cloudHealthCheckItems = ref<CloudHealthCheckItem[]>([]);
const currentTime = ref(formatCurrentTime(new Date()));
let clockTimer: number | undefined;
const cloudLibrary = computed(() => cloudLibraryIndicator(cloudSettings, cloudHealth.value));
const cloudStateClass = computed(() => {
  if (!cloudSettings.enabled || !cloudSettings.sources.length) return "unavailable";
  return cloudHealth.value.status === "unavailable" ? "failed" : cloudHealth.value.status;
});

function refreshCurrentTime(): void {
  const now = new Date();
  currentTime.value = formatCurrentTime(now);
  window.clearTimeout(clockTimer);
  clockTimer = window.setTimeout(refreshCurrentTime, millisecondsUntilNextMinute(now));
}

async function checkCloudSources(showDialog = false): Promise<void> {
  if (!cloudSettings.enabled || !cloudSettings.sources.length) {
    cloudHealth.value = { status: "unchecked" };
    cloudHealthCheckItems.value = [];
    if (showDialog) cloudHealthDialogOpen.value = true;
    return;
  }
  if (showDialog) cloudHealthDialogOpen.value = true;
  cloudHealth.value = { status: "checking" };
  cloudHealthCheckRunning.value = true;
  const items = await runCloudHealthCheck(cloudSettings.sources, async (source) => {
    await cloudRepository.test(source, "");
    recordCloudSourceTest(source);
  }, (next) => { cloudHealthCheckItems.value = next; });
  cloudHealthCheckRunning.value = false;
  const failed = items.find((item) => item.status === "failed");
  cloudHealth.value = failed ? { status: "unavailable", sourceName: failed.name, reason: failed.reason } : { status: "available" };
}

function openCloudHealthDialog(): void {
  if (!cloudHealthCheckRunning.value) void checkCloudSources(true);
  else cloudHealthDialogOpen.value = true;
}

function queueAutomaticUpload(archive?: ArchiveRecord): void {
  const sources = enabledCloudSources(cloudSettings);
  if (!archive || !isTauriRuntime || archive.storagePolicy !== "local_and_remote" || archive.syncMode !== "automatic" || !sources.length) return;
  window.clearTimeout(automaticSyncTimers.get(archive.id));
  automaticSyncTimers.set(archive.id, window.setTimeout(async () => {
    automaticSyncTimers.delete(archive.id);
    const outcomes = await runAcrossEnabledSources(sources, (source) => cloudRepository.upload(source.id, archive.id));
    reportSourceFailures(outcomes, archive, "自动上传");
    const completed = outcomes.filter((outcome) => outcome.status === "fulfilled").length;
    if (completed) showNotice(`“${archive.name}”已自动上传到 ${completed} 个同步源`);
  }, 1200));
}

const descendantIds = (categoryId: string): Set<string> => {
  const result = new Set([categoryId]);
  for (const category of categoryRecords.value) {
    if (category.parentId && result.has(category.parentId)) result.add(category.id);
  }
  return result;
};
const categoryTreeNodes = computed<Array<CategoryTreeNode | ArchiveTreeNode>>(() => {
  const rows: Array<CategoryTreeNode | ArchiveTreeNode> = [];
  const append = (parentId: string | null, depth: number) => {
    const childCategories = categoryRecords.value
      .filter((category) => (category.parentId ?? null) === parentId)
      .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));

    childCategories.forEach((category) => {
      const ids = descendantIds(category.id);
      const directArchives = archives.value
        .filter((archive) => archive.categoryId === category.id)
        .sort((left, right) => compareArchiveNames(left.name, right.name));
      rows.push({
        ...category,
        nodeType: "category",
        depth,
        count: archives.value.filter((archive) => archive.categoryId && ids.has(archive.categoryId)).length,
        icon: Folder,
        hasChildren: categoryRecords.value.some((item) => item.parentId === category.id) || directArchives.length > 0,
      });
      if (!expandedCategoryIds.value.has(category.id)) return;
      append(category.id, depth + 1);
      directArchives.forEach((archive) => rows.push({ nodeType: "archive", id: archive.id, depth: depth + 1, archive }));
    });
  };
  append(null, 0);
  return [{ nodeType: "category", id: "all", name: "全部存档", depth: 0, count: archives.value.length, icon: FolderArchive, hasChildren: false }, ...rows];
});
const selectedCategoryName = computed(() => categoryRecords.value.find((category) => category.id === selectedCategoryId.value)?.name ?? "全部存档");
const selectedCategoryPath = computed(() => categoryBreadcrumb(categoryRecords.value, selectedCategoryId.value));
const filteredArchives = computed(() => {
  const query = searchTerm.value.trim().toLocaleLowerCase();
  const selectedIds = selectedCategoryId.value === "all" ? undefined : descendantIds(selectedCategoryId.value);
  const filtered = archives.value.filter((archive) =>
    (!selectedIds || Boolean(archive.categoryId && selectedIds.has(archive.categoryId))) &&
    (!query || `${archive.name} ${archive.sources.map((source) => `${source.name} ${source.path}`).join(" ")} ${archive.category} ${archive.tags.join(" ")}`.toLocaleLowerCase().includes(query)),
  );
  return filtered.sort((left, right) => {
    if (sortMode.value === "nameAsc") return compareArchiveNames(left.name, right.name);
    if (sortMode.value === "nameDesc") return compareArchiveNames(left.name, right.name, true);
    return sortMode.value === "newest" ? right.updatedAt - left.updatedAt : left.updatedAt - right.updatedAt;
  });
});
const selectedArchive = computed(() => archives.value.find((archive) => archive.id === selectedArchiveId.value));
const selectedSnapshot = computed(() => snapshots.value.find((snapshot) => snapshot.id === selectedSnapshotId.value));
const visibleSnapshots = computed(() => filterTimeline(snapshots.value, snapshotSearch.value, snapshotSort.value));
const snapshotButtonState = computed(() => snapshotButtonProgress(snapshotProgress.value));

function showNotice(message: string, type: "success" | "error" | "info" = "success") {
  notice.value = { message, type };
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => { notice.value = undefined; }, 3200);
}

function readableError(error: unknown): string {
  return diagnosticFromError(error, { operation: "应用操作" }).message;
}

function reportError(error: unknown, context: DiagnosticContext): void {
  const entry = diagnosticFromError(error, context);
  showNotice(entry.message, "error");
  void diagnosticsRepository.record(error, context).catch(() => undefined);
}

function reportSourceFailures(outcomes: SourceSyncOutcome<unknown>[], archive: ArchiveRecord, operation: string): void {
  for (const outcome of outcomes) {
    if (outcome.status === "rejected") reportError(outcome.reason, { operation, archiveId: archive.id, sourceId: outcome.source.id });
  }
}

function formatBytes(bytes: number): string {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** unit).toFixed(unit > 1 ? 1 : 0)} ${units[unit]}`;
}

function formatTime(timestamp?: number): string {
  if (!timestamp) return "尚未备份";
  const date = new Date(timestamp);
  const today = new Date();
  if (date.toDateString() === today.toDateString()) {
    return `今天 ${date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false })}`;
  }
  return date.toLocaleString("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false });
}

function displaySourcePath(path: string): string {
  return path.split(" · ").map((part) => part.startsWith("\\\\?\\") ? part.slice(4) : part).join(" · ");
}

function snapshotDetail(snapshot: SnapshotRecord): string {
  const { added, modified, deleted } = snapshot.changes;
  if (!added && !modified && !deleted) return "内容与上一个时间节点一致";
  return [`新增 ${added}`, `修改 ${modified}`, `删除 ${deleted}`].filter((part) => !part.endsWith(" 0")).join(" · ");
}

async function refreshArchives(preferredId?: string) {
  archives.value = await archiveRepository.listArchives();
  if (preferredId) selectedArchiveId.value = preferredId;
  if (!selectedArchive.value) selectedArchiveId.value = filteredArchives.value[0]?.id ?? archives.value[0]?.id;
}

async function refreshCategories() {
  categoryRecords.value = await archiveRepository.listCategories();
}

async function refreshRepositoryInfo() {
  repositoryInfo.value = await archiveRepository.getRepositoryInfo();
}

async function handleSettingsChanged() {
  applyAppearance(appSettings);
  await Promise.all([refreshArchives(), refreshCategories(), refreshRepositoryInfo()]);
  showNotice("设置已保存");
}

function handleCloudSettingsChanged(): void {
  cloudHealth.value = { status: "unchecked" };
  cloudHealthCheckItems.value = [];
  showNotice("云端设置已保存");
}

async function openRepositoryFolder() {
  try {
    await archiveRepository.openRepositoryFolder();
  } catch (error) {
    reportError(error, { operation: "打开资料库" });
  }
}

async function openSelectedArchiveStorage() {
  const archive = selectedArchive.value;
  if (!archive) return;
  try {
    await archiveRepository.openArchiveStorage(archive.id);
  } catch (error) {
    showNotice(readableError(error), "error");
  }
}

async function openSelectedArchiveSources(): Promise<void> {
  const archive = selectedArchive.value;
  if (!archive) return;
  try {
    await archiveRepository.openArchiveSources(archive.id);
  } catch (error) {
    reportError(error, { operation: "打开存档来源", archiveId: archive.id });
  }
}

async function refreshSnapshots(archiveId?: string) {
  snapshots.value = archiveId ? await archiveRepository.listSnapshots(archiveId) : [];
  selectedSnapshotId.value = snapshots.value[0]?.id;
}

function openCreateArchive() {
  editingArchive.value = undefined;
  pendingSources.value = [];
  createArchiveError.value = undefined;
  createDialogOpen.value = true;
}

function openEditArchive() {
  if (!selectedArchive.value) return;
  editingArchive.value = selectedArchive.value;
  pendingSources.value = selectedArchive.value.sources.map((source) => ({ ...source }));
  createArchiveError.value = undefined;
  archiveMenuOpen.value = false;
  createDialogOpen.value = true;
}

function closeArchiveDialog() {
  createDialogOpen.value = false;
  editingArchive.value = undefined;
}

function requestConfirmation(title: string, message: string, confirmLabel: string, destructive = false): Promise<boolean> {
  confirmRequest.value = { title, message, confirmLabel, destructive };
  return new Promise((resolve) => { confirmResolver = resolve; });
}

function answerConfirmation(confirmed: boolean) {
  confirmRequest.value = undefined;
  confirmResolver?.(confirmed);
  confirmResolver = undefined;
}

async function pickSources(kind: SourceKind) {
  if (!isTauriRuntime && (!("showOpenFilePicker" in window) || !("showDirectoryPicker" in window))) {
    showNotice("当前环境不支持本地文件系统访问，请使用 Edge 或桌面版 Chronicle", "error");
    return;
  }
  pickingSource.value = kind;
  try {
    const selected = await archiveRepository.pickSources(kind);
    const paths = new Set(pendingSources.value.map((source) => `${source.kind}:${source.path}`));
    pendingSources.value.push(...selected.filter((source) => !paths.has(`${source.kind}:${source.path}`)));
    createArchiveError.value = undefined;
  } catch (error) {
    createArchiveError.value = readableError(error);
  } finally {
    pickingSource.value = undefined;
  }
}

async function createArchive(input: CreateArchiveInput) {
  creatingArchive.value = true;
  createArchiveError.value = undefined;
  try {
    if (editingArchive.value) {
      const archiveId = editingArchive.value.id;
      await archiveRepository.updateArchive(archiveId, input);
      closeArchiveDialog();
      await refreshArchives(archiveId);
      await refreshRepositoryInfo();
      queueAutomaticUpload(archives.value.find((archive) => archive.id === archiveId));
      showNotice("存档设置已更新");
      return;
    }
    const categoryId = selectedCategoryId.value === "all" ? undefined : selectedCategoryId.value;
    const archive = await archiveRepository.createArchive({ ...input, categoryId });
    createDialogOpen.value = false;
    if (categoryId) expandedCategoryIds.value = new Set([...expandedCategoryIds.value, categoryId]);
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    await refreshRepositoryInfo();
    if (input.createInitialSnapshot) {
      await createSnapshot("初始版本");
    } else {
      showNotice(`已创建存档“${archive.name}”`);
    }
  } catch (error) {
    createArchiveError.value = readableError(error);
  } finally {
    creatingArchive.value = false;
  }
}

async function deleteArchive(archive: ArchiveRecord) {
  archiveMenuOpen.value = false;
  const action = appSettings.recycleBinEnabled ? "移入回收站" : "永久删除";
  const confirmed = await requestConfirmation(
    `${action}“${archive.name}”`,
    appSettings.recycleBinEnabled ? "存档及其全部时间节点将移入回收站。" : "存档及其全部时间节点将被永久删除，无法恢复。",
    action,
    true,
  );
  if (!confirmed) return;
  try {
    await archiveRepository.deleteArchive(archive.id, appSettings.recycleBinEnabled, appSettings.recycleBinPath);
    selectedArchiveId.value = undefined;
    activeTreeNodeId.value = `category:${selectedCategoryId.value}`;
    await refreshArchives();
    await refreshCategories();
    await refreshRepositoryInfo();
    showNotice(appSettings.recycleBinEnabled ? "存档已移入回收站" : "存档已永久删除");
  } catch (error) {
    showNotice(readableError(error), "error");
  }
}

async function deleteCategory(categoryId: string) {
  const category = categoryRecords.value.find((item) => item.id === categoryId);
  if (!category) return;
  const ids = descendantIds(categoryId);
  const archiveCount = archives.value.filter((archive) => archive.categoryId && ids.has(archive.categoryId)).length;
  const action = appSettings.recycleBinEnabled ? "移入回收站" : "永久删除";
  const confirmed = await requestConfirmation(
    `${action}分类“${category.name}”`,
    `该分类的子分类和 ${archiveCount} 个存档将一并${appSettings.recycleBinEnabled ? "移入回收站" : "永久删除"}。`,
    action,
    true,
  );
  if (!confirmed) return;
  try {
    await archiveRepository.deleteCategory(categoryId, appSettings.recycleBinEnabled, appSettings.recycleBinPath);
    selectedCategoryId.value = "all";
    selectedArchiveId.value = undefined;
    activeTreeNodeId.value = "category:all";
    await refreshCategories();
    await refreshArchives();
    await refreshRepositoryInfo();
    showNotice(appSettings.recycleBinEnabled ? "分类已移入回收站" : "分类已永久删除");
  } catch (error) {
    showNotice(readableError(error), "error");
  }
}

function archiveMoveOptions(): ThemedSelectOption[] {
  return [
    { value: null, label: "根目录" },
    ...categoryRecords.value.map((category) => ({ value: category.id, label: category.name })),
  ];
}

function categoryMoveOptions(categoryId: string): ThemedSelectOption[] {
  const excluded = descendantIds(categoryId);
  return [
    { value: null, label: "根目录" },
    ...categoryRecords.value
      .filter((category) => !excluded.has(category.id))
      .map((category) => ({ value: category.id, label: category.name })),
  ];
}

function toggleTreeMenu(kind: "archive" | "category", id: string, event: MouseEvent): void {
  if (treeMenu.value?.kind === kind && treeMenu.value.id === id) {
    treeMenu.value = undefined;
    return;
  }
  const button = event.currentTarget;
  if (!(button instanceof HTMLElement)) return;
  treeMenuAnchor.value = button.getBoundingClientRect();
  treeMenu.value = { kind, id };
}

watch(treeMenu, (value, _, onCleanup) => {
  if (!value) return;
  const close = () => { treeMenu.value = undefined; };
  window.addEventListener("scroll", close, true);
  window.addEventListener("resize", close);
  onCleanup(() => {
    window.removeEventListener("scroll", close, true);
    window.removeEventListener("resize", close);
  });
});

async function moveArchiveFromMenu(archiveId: string, value: string | null) {
  try {
    await archiveRepository.setArchiveCategory(archiveId, value ?? undefined);
    treeMenu.value = undefined;
    await refreshArchives(archiveId);
  } catch (error) { showNotice(readableError(error), "error"); }
}

async function moveCategoryFromMenu(categoryId: string, value: string | null) {
  try {
    await archiveRepository.moveCategory(categoryId, value ?? undefined);
    treeMenu.value = undefined;
    await refreshCategories();
  } catch (error) { showNotice(readableError(error), "error"); }
}

async function createSnapshot(title = "手动备份") {
  if (!selectedArchive.value || busyAction.value) return;
  busyAction.value = "snapshot";
  snapshotProgress.value = { current: 0, total: 0, currentPath: "正在扫描文件" };
  try {
    const snapshot = await archiveRepository.createSnapshot(
      selectedArchive.value, title, false,
      (progress) => { snapshotProgress.value = progress; },
    );
    await refreshArchives(selectedArchive.value.id);
    await refreshSnapshots(selectedArchive.value.id);
    await refreshRepositoryInfo();
    selectedSnapshotId.value = snapshot.id;
    queueAutomaticUpload(archives.value.find((archive) => archive.id === selectedArchive.value?.id));
    showNotice(`时间节点已创建，保存 ${snapshot.files.length} 个文件`);
  } catch (error) {
    reportError(error, { operation: "创建备份", archiveId: selectedArchive.value?.id });
  } finally {
    busyAction.value = undefined;
    snapshotProgress.value = undefined;
  }
}

async function syncSelectedArchive() {
  const archive = selectedArchive.value;
  if (!archive || syncingArchive.value) return;
  if (!isTauriRuntime) {
    showNotice("云同步仅在 Chronicle 桌面端可用", "error");
    return;
  }
  if (archive.storagePolicy !== "local_and_remote") {
    showNotice("当前存档仅使用本地存储，请先在编辑存档中启用云端保存", "info");
    return;
  }
  const sources = enabledCloudSources(cloudSettings);
  if (!sources.length) {
    cloudSettingsOpen.value = true;
    showNotice("请先添加同步源，并点击开始同步", "info");
    return;
  }
  syncingArchive.value = true;
  try {
    const outcomes = await runAcrossEnabledSources(sources, (source) => cloudRepository.sync(source.id, archive.id));
    reportSourceFailures(outcomes, archive, "同步存档");
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    const completed = outcomes.filter((outcome): outcome is Extract<SourceSyncOutcome<CloudSyncResult>, { status: "fulfilled" }> => outcome.status === "fulfilled");
    if (completed.length) {
      const hasConflict = completed.some((outcome) => outcome.value.status === "conflict");
      showNotice(`已完成 ${completed.length} / ${outcomes.length} 个同步源`, hasConflict ? "info" : "success");
    }
  } catch (error) {
    reportError(error, { operation: "同步存档", archiveId: archive.id });
  } finally {
    syncingArchive.value = false;
  }
}

async function restoreSnapshot() {
  const archive = selectedArchive.value;
  const snapshot = selectedSnapshot.value;
  if (!archive || !snapshot || busyAction.value) return;
  const confirmed = window.confirm(`将“${archive.name}”恢复到 ${formatTime(snapshot.createdAt)}。Chronicle 会先保存当前状态，是否继续？`);
  if (!confirmed) return;
  busyAction.value = "restore";
  try {
    await archiveRepository.restoreSnapshot(archive, snapshot);
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    showNotice("恢复完成，原状态已保存为安全快照");
  } catch (error) {
    showNotice(readableError(error), "error");
  } finally {
    busyAction.value = undefined;
  }
}

async function saveSnapshotNote(): Promise<void> {
  const archive = selectedArchive.value;
  const snapshot = selectedSnapshot.value;
  if (!archive || !snapshot || savingSnapshotNote.value) return;
  savingSnapshotNote.value = true;
  try {
    const updated = await archiveRepository.updateSnapshotNote(archive.id, snapshot.id, snapshotNote.value);
    snapshots.value = snapshots.value.map((item) => item.id === updated.id ? updated : item);
    queueAutomaticUpload(archives.value.find((item) => item.id === archive.id));
    showNotice("快照备注已保存");
  } catch (error) {
    reportError(error, { operation: "保存快照备注", archiveId: archive.id });
  } finally {
    savingSnapshotNote.value = false;
  }
}

async function deleteSnapshot(snapshot: SnapshotRecord): Promise<void> {
  const archive = selectedArchive.value;
  if (!archive || busyAction.value) return;
  const confirmed = await requestConfirmation(
    "永久删除时间节点",
    `将永久删除 ${formatTime(snapshot.createdAt)} 的快照文件及其备注，无法恢复。`,
    "永久删除",
    true,
  );
  if (!confirmed) return;
  try {
    await archiveRepository.deleteSnapshot(archive.id, snapshot.id);
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    await refreshRepositoryInfo();
    queueAutomaticUpload(archives.value.find((item) => item.id === archive.id));
    showNotice("时间节点已永久删除");
  } catch (error) {
    reportError(error, { operation: "删除时间节点", archiveId: archive.id });
  }
}

async function toggleColorMode(): Promise<void> {
  const appearance = normalizeAppearance(appSettings, true);
  try {
    await saveAppSettings({ ...appSettings, ...appearance });
    applyAppearance(appSettings);
  } catch (error) {
    showNotice(readableError(error), "error");
  }
}

function selectCategory(categoryId: string) {
  const panelState = selectCategoryPanel({ categoryId: selectedCategoryId.value, collapsed: archivePanelCollapsed.value }, categoryId);
  selectedCategoryId.value = panelState.categoryId;
  archivePanelCollapsed.value = panelState.collapsed;
  activeTreeNodeId.value = `category:${categoryId}`;
  if (!panelState.collapsed) selectedArchiveId.value = filteredArchives.value[0]?.id;
}

function selectArchive(archiveId: string) {
  selectedArchiveId.value = archiveId;
  activeTreeNodeId.value = `archive:${archiveId}`;
}

function selectArchiveFromTree(archive: ArchiveRecord) {
  const panelState = selectArchivePanelCategory(archive.categoryId);
  selectedCategoryId.value = panelState.categoryId;
  archivePanelCollapsed.value = panelState.collapsed;
  selectArchive(archive.id);
}

function createSnapshotFromDetail(): void {
  const title = snapshotDescription.value.trim() || "手动备份";
  snapshotDescription.value = "";
  void createSnapshot(title);
}

function updateTimelineSort(value: string | null): void {
  if (value === "newest" || value === "oldest") snapshotSort.value = value;
}

function openCategoryDialog() {
  createCategoryError.value = undefined;
  categoryDialogOpen.value = true;
}

function toggleCategory(categoryId: string) {
  const next = new Set(expandedCategoryIds.value);
  if (next.has(categoryId)) next.delete(categoryId);
  else next.add(categoryId);
  expandedCategoryIds.value = next;
}

async function createCategory(name: string) {
  const parent = categoryRecords.value.find((category) => category.id === selectedCategoryId.value);
  creatingCategory.value = true;
  createCategoryError.value = undefined;
  try {
    const category = await archiveRepository.createCategory(name, parent?.id);
    await refreshCategories();
    if (parent) expandedCategoryIds.value = new Set([...expandedCategoryIds.value, parent.id]);
    selectedCategoryId.value = category.id;
    activeTreeNodeId.value = `category:${category.id}`;
    categoryDialogOpen.value = false;
  } catch (error) {
    createCategoryError.value = readableError(error);
  } finally {
    creatingCategory.value = false;
  }
}

async function moveArchiveToCategory(categoryId?: string) {
  const archiveId = draggedArchiveId.value;
  draggedArchiveId.value = undefined;
  categoryDropTarget.value = undefined;
  if (!archiveId) return;
  try {
    await archiveRepository.setArchiveCategory(archiveId, categoryId);
    if (categoryId) expandedCategoryIds.value = new Set([...expandedCategoryIds.value, categoryId]);
    await refreshArchives(archiveId);
    const name = categoryRecords.value.find((category) => category.id === categoryId)?.name;
    showNotice(name ? `存档已移入“${name}”` : "存档已移至根目录");
  } catch (error) {
    showNotice(readableError(error), "error");
  }
}

async function replaceArchiveTags(tags: string[]) {
  const archive = selectedArchive.value;
  if (!archive || savingTags.value) return;
  savingTags.value = true;
  try {
    await archiveRepository.setArchiveTags(archive.id, tags);
    await refreshArchives(archive.id);
    showNotice("标签已更新");
  } catch (error) {
    showNotice(readableError(error), "error");
  } finally {
    savingTags.value = false;
  }
}

function addArchiveTag(tag: string) {
  const tags = selectedArchive.value?.tags ?? [];
  if (tags.length >= 20) {
    showNotice("每个存档最多添加 20 个标签", "error");
    return;
  }
  void replaceArchiveTags([...tags, tag]);
}

function removeArchiveTag(tag: string) {
  void replaceArchiveTags((selectedArchive.value?.tags ?? []).filter((item) => item !== tag));
}

function startArchiveDrag(archiveId: string, event: DragEvent) {
  draggedCategoryId.value = undefined;
  draggedArchiveId.value = archiveId;
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", `archive:${archiveId}`);
    event.dataTransfer.setData("application/x-chronicle-archive", archiveId);
  }
}

function startCategoryDrag(categoryId: string, event: DragEvent) {
  draggedArchiveId.value = undefined;
  draggedCategoryId.value = categoryId;
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", `category:${categoryId}`);
    event.dataTransfer.setData("application/x-chronicle-category", categoryId);
  }
}

function finishDrag() {
  draggedArchiveId.value = undefined;
  draggedCategoryId.value = undefined;
  categoryDropTarget.value = undefined;
  trashDropActive.value = false;
}

function handleTrashDragOver(event: DragEvent) {
  if (!draggedArchiveId.value && !draggedCategoryId.value) return;
  event.preventDefault();
  trashDropActive.value = true;
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
}

async function dropInTrash() {
  const archive = archives.value.find((item) => item.id === draggedArchiveId.value);
  const categoryId = draggedCategoryId.value;
  finishDrag();
  if (archive) await deleteArchive(archive);
  else if (categoryId) await deleteCategory(categoryId);
}

function canDropOnCategory(categoryId: string): boolean {
  if (categoryId === "all") {
    if (draggedCategoryId.value) return Boolean(categoryRecords.value.find((category) => category.id === draggedCategoryId.value)?.parentId);
    return Boolean(archives.value.find((archive) => archive.id === draggedArchiveId.value)?.categoryId);
  }
  if (!draggedCategoryId.value) return Boolean(draggedArchiveId.value);
  return categoryId !== draggedCategoryId.value && !descendantIds(draggedCategoryId.value).has(categoryId);
}

function handleCategoryDragOver(categoryId: string, event: DragEvent) {
  if (!canDropOnCategory(categoryId)) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  categoryDropTarget.value = categoryId;
}

async function dropOnCategory(categoryId: string) {
  if (!canDropOnCategory(categoryId)) return;
  const parentId = categoryId === "all" ? undefined : categoryId;
  if (draggedCategoryId.value) {
    const categoryIdToMove = draggedCategoryId.value;
    const movedName = categoryRecords.value.find((category) => category.id === categoryIdToMove)?.name ?? "分类";
    try {
      await archiveRepository.moveCategory(categoryIdToMove, parentId);
      if (parentId) expandedCategoryIds.value = new Set([...expandedCategoryIds.value, parentId]);
      await refreshCategories();
      showNotice(parentId ? `“${movedName}”已移入目标分类` : `“${movedName}”已移至根目录`);
    } catch (error) {
      showNotice(readableError(error), "error");
    } finally {
      finishDrag();
    }
    return;
  }
  await moveArchiveToCategory(parentId);
}

function handleShortcut(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (confirmRequest.value) {
      answerConfirmation(false);
      return;
    }
    createDialogOpen.value = false;
    editingArchive.value = undefined;
    categoryDialogOpen.value = false;
    sortMenuOpen.value = false;
    settingsOpen.value = false;
    cloudSettingsOpen.value = false;
    return;
  }
  if (settingsOpen.value || cloudSettingsOpen.value || createDialogOpen.value || categoryDialogOpen.value) return;
  const target = event.target as HTMLElement | null;
  const editing = target?.matches("input, textarea, select, [contenteditable='true']");
  if (shortcutMatches(event, appSettings.settingsShortcut)) {
    event.preventDefault();
    settingsOpen.value = true;
    return;
  }
  if (editing) return;
  if (shortcutMatches(event, appSettings.searchShortcut)) {
    event.preventDefault();
    void nextTick(() => searchInput.value?.focus());
  }
  if (shortcutMatches(event, appSettings.snapshotShortcut)) {
    event.preventDefault();
    void createSnapshot();
  }
}

watch(selectedArchiveId, (archiveId) => { activityPanelOpen.value = false; void refreshSnapshots(archiveId); });
watch(selectedSnapshot, (snapshot) => {
  snapshotNote.value = snapshot?.note ?? "";
  if (!snapshot) activityPanelOpen.value = false;
});
onMounted(async () => {
  window.addEventListener("keydown", handleShortcut);
  refreshCurrentTime();
  try {
    await initializeSettings();
    applyAppearance(appSettings);
    await refreshCategories();
    await refreshArchives();
    await refreshRepositoryInfo();
    if (appSettings.checkCloudOnLaunch && isTauriRuntime) void checkCloudSources();
  }
  catch (error) { showNotice(readableError(error), "error"); }
  finally { loading.value = false; }
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleShortcut);
  window.clearTimeout(noticeTimer);
  window.clearTimeout(clockTimer);
  automaticSyncTimers.forEach((timer) => window.clearTimeout(timer));
});
</script>

<template>
  <div class="app-shell">
    <header class="titlebar">
      <div class="brand"><time :datetime="currentTime">{{ currentTime }}</time></div>
      <div class="sync-states"><button class="sync-state sync-state-button" title="打开本地资料库" @click="openRepositoryFolder"><i></i>本地资料库可用</button><button class="sync-state sync-state-button" :class="cloudStateClass" :title="cloudHealth.reason || '检测云端资料库'" @click="openCloudHealthDialog"><i :class="{ pulse: cloudHealth.status === 'checking' }"></i>{{ cloudLibrary.label }}</button></div>
      <div class="toolbar"><button class="toolbar-action mode-toggle" :class="{ 'is-dark': appSettings.colorMode === 'dark' }" :aria-label="appSettings.colorMode === 'dark' ? '切换到日间模式' : '切换到夜间模式'" :title="appSettings.colorMode === 'dark' ? '切换到日间模式' : '切换到夜间模式'" :aria-pressed="appSettings.colorMode === 'dark'" @click="toggleColorMode"><Sun v-if="appSettings.colorMode === 'dark'" :size="17" /><Moon v-else :size="17" /></button><button class="toolbar-action" aria-label="云端设置" title="云端设置" @click="cloudSettingsOpen = true"><CloudCog :size="17" /></button><button class="toolbar-action" aria-label="应用设置" title="应用设置" @click="settingsOpen = true"><Settings2 :size="17" /></button></div>
    </header>

    <aside class="sidebar">
      <div class="add-control"><button class="add-button" @click="openCreateArchive"><Plus :size="18" />添加存档</button></div>
      <nav aria-label="存档分类">
        <div class="nav-heading"><p class="label">资料库</p><button aria-label="添加分类" title="添加分类" @click="openCategoryDialog"><Plus :size="15" /></button></div>
        <template v-for="node in categoryTreeNodes" :key="`${node.nodeType}:${node.id}`">
          <div
            v-if="node.nodeType === 'category'"
            class="category-tree-row"
            :class="{ active: activeTreeNodeId === `category:${node.id}`, 'drop-target': categoryDropTarget === node.id }"
            :style="{ paddingLeft: `${4 + node.depth * 16}px` }"
            @dragend="finishDrag"
            @dragover="handleCategoryDragOver(node.id, $event)"
            @dragleave="categoryDropTarget === node.id && (categoryDropTarget = undefined)"
            @drop.prevent="dropOnCategory(node.id)"
          >
            <button v-if="node.id !== 'all'" class="disclosure" :class="{ hidden: !node.hasChildren }" :aria-label="`${expandedCategoryIds.has(node.id) ? '折叠' : '展开'} ${node.name}`" :title="`${expandedCategoryIds.has(node.id) ? '折叠' : '展开'} ${node.name}`" :aria-expanded="node.hasChildren ? expandedCategoryIds.has(node.id) : undefined" @click="toggleCategory(node.id)"><ChevronDown v-if="expandedCategoryIds.has(node.id)" :size="14" /><ChevronRight v-else :size="14" /></button>
            <span v-else class="disclosure-spacer"></span>
            <button class="category-select" :draggable="node.id !== 'all'" :title="node.id === 'all' ? '将分类或存档拖到这里可移至根目录' : undefined" @dragstart.stop="node.id !== 'all' && startCategoryDrag(node.id, $event)" @click="selectCategory(node.id)"><component :is="node.icon" :size="17" /><span>{{ node.name }}</span><span class="category-suffix"><LockKeyhole v-if="node.id === 'all'" :size="12" aria-label="固定根目录" /><small>{{ node.count }}</small></span></button>
            <div v-if="node.id !== 'all'" class="tree-more"><button aria-label="分类操作" title="分类操作" :aria-expanded="treeMenu?.kind === 'category' && treeMenu.id === node.id" @click="toggleTreeMenu('category', node.id, $event)"><MoreHorizontal :size="14" /></button><Teleport to="body"><div v-if="treeMenu?.kind === 'category' && treeMenu.id === node.id" class="tree-menu" :style="treeMenuStyle"><label>移动到<ThemedSelect :model-value="node.parentId ?? null" :options="categoryMoveOptions(node.id)" :label="`移动分类 ${node.name}`" @update:model-value="moveCategoryFromMenu(node.id, $event)" /></label><button class="danger" @click="deleteCategory(node.id)"><Trash2 :size="13" />删除分类</button></div></Teleport></div>
          </div>
          <div v-else class="archive-tree-row" :class="{ active: activeTreeNodeId === `archive:${node.id}` }" :style="{ paddingLeft: `${4 + node.depth * 16}px` }" @dragend="finishDrag">
            <span class="disclosure-spacer"></span>
            <button class="archive-tree-select" draggable="true" :title="node.archive.name" @dragstart.stop="startArchiveDrag(node.id, $event)" @click="selectArchiveFromTree(node.archive)"><File :size="16" /><span>{{ node.archive.name }}</span></button>
            <div class="tree-more"><button aria-label="存档移动操作" title="存档移动操作" :aria-expanded="treeMenu?.kind === 'archive' && treeMenu.id === node.id" @click="toggleTreeMenu('archive', node.id, $event)"><MoreHorizontal :size="14" /></button><Teleport to="body"><div v-if="treeMenu?.kind === 'archive' && treeMenu.id === node.id" class="tree-menu" :style="treeMenuStyle"><label>移动到<ThemedSelect :model-value="node.archive.categoryId ?? null" :options="archiveMoveOptions()" :label="`移动存档 ${node.archive.name}`" @update:model-value="moveArchiveFromMenu(node.id, $event)" /></label><button class="danger" @click="deleteArchive(node.archive)"><Trash2 :size="13" />删除存档</button></div></Teleport></div>
          </div>
        </template>
      </nav>
      <div class="spacer"></div>
      <div v-if="draggedArchiveId || draggedCategoryId" class="trash-drop-zone" :class="{ active: trashDropActive }" @dragover="handleTrashDragOver" @dragleave="trashDropActive = false" @drop.prevent="dropInTrash"><Trash2 :size="19" /><span><b>{{ appSettings.recycleBinEnabled ? '移入回收站' : '永久删除' }}</b><small>拖到这里后松开</small></span></div>
      <section class="storage"><div><HardDrive :size="17" /><span>本地存储</span><b>{{ formatBytes(repositoryInfo.totalBytes) }}</b><button aria-label="打开本地资料库文件夹" title="打开本地资料库文件夹" @click="openRepositoryFolder"><FolderOpen :size="14" /></button></div><small :title="repositoryInfo.path">{{ repositoryInfo.path || 'Chronicle 本地资料库' }}</small></section>
      <div class="account"><span class="avatar">T</span><span><b>ThermalEX</b><small>本机设备</small></span></div>
    </aside>

    <main class="workspace" :class="{ 'archive-panel-collapsed': archivePanelCollapsed }">
      <section v-show="!archivePanelCollapsed" class="archive-panel" aria-labelledby="archives-title">
        <div class="panel-title"><div><h1 id="archives-title">{{ selectedCategoryName }}</h1><p class="category-path">{{ selectedCategoryPath }}</p></div><div class="sort-control"><button class="icon-button" aria-label="排列方式" title="排列方式" :aria-expanded="sortMenuOpen" @click="sortMenuOpen = !sortMenuOpen"><SlidersHorizontal :size="18" /></button><div v-if="sortMenuOpen" class="sort-menu"><button :class="{ active: sortMode === 'newest' }" @click="sortMode = 'newest'; sortMenuOpen = false">时间 新–旧</button><button :class="{ active: sortMode === 'oldest' }" @click="sortMode = 'oldest'; sortMenuOpen = false">时间 旧–新</button><button :class="{ active: sortMode === 'nameAsc' }" @click="sortMode = 'nameAsc'; sortMenuOpen = false">名称 A–Z</button><button :class="{ active: sortMode === 'nameDesc' }" @click="sortMode = 'nameDesc'; sortMenuOpen = false">名称 Z–A</button></div></div></div>
        <label class="search"><Search :size="17" /><input ref="searchInput" v-model="searchTerm" type="search" placeholder="搜索名称、来源或标签" /><kbd>Ctrl K</kbd></label>
        <div class="archive-list" :aria-busy="loading">
          <button v-for="item in filteredArchives" :key="item.id" class="archive-row" :class="{ selected: selectedArchiveId === item.id }" draggable="true" @dragstart="startArchiveDrag(item.id, $event)" @dragend="finishDrag" @click="selectArchive(item.id)">
            <span class="file-icon"><Folder v-if="item.kind === 'folder'" :size="19" /><File v-else-if="item.kind === 'file'" :size="19" /><FolderArchive v-else :size="19" /></span>
            <span class="archive-copy"><span class="row-title"><b>{{ item.name }}</b><i :class="item.lastSnapshotAt ? 'synced' : 'local'"><Check v-if="item.lastSnapshotAt" :size="13" /><HardDrive v-else :size="13" /></i></span><small>{{ displaySourcePath(item.sourcePath) }}</small><span class="meta"><span>{{ item.category }}</span><span>{{ formatBytes(item.totalBytes) }}</span><span>{{ formatTime(item.lastSnapshotAt) }}</span></span></span>
          </button>
          <div v-if="!loading && !archives.length" class="empty-state"><span class="empty-icon"><FolderArchive :size="26" /></span><b>添加第一个存档</b><p>把一个或多个文件、文件夹组合为可查询和恢复的时间线。</p><button @click="openCreateArchive"><Plus :size="16" />添加存档</button></div>
          <div v-else-if="!loading && !filteredArchives.length" class="empty"><Search :size="22" /><span>没有找到匹配的存档</span></div>
        </div>
        <button class="archive-panel-toggle" type="button" aria-label="收起存档列表" title="收起存档列表" :aria-expanded="true" @click="archivePanelCollapsed = true"><ChevronLeft :size="16" /></button>
      </section>
      <button v-if="archivePanelCollapsed" class="archive-panel-toggle collapsed" type="button" aria-label="展开存档列表" title="展开存档列表" :aria-expanded="false" @click="archivePanelCollapsed = false"><ChevronRight :size="16" /></button>

      <section v-if="selectedArchive" class="detail-panel" aria-labelledby="detail-title">
        <header class="detail-header">
          <div class="identity"><span class="detail-icon"><Folder v-if="selectedArchive.kind === 'folder'" /><File v-else-if="selectedArchive.kind === 'file'" /><FolderArchive v-else /></span><div class="title-line"><h2 id="detail-title">{{ selectedArchive.name }}</h2></div></div>
          <div class="actions"><button class="secondary" :disabled="syncingArchive" @click="syncSelectedArchive"><UploadCloud :size="17" />{{ syncingArchive ? '同步中' : '同步' }}</button><div class="more-control"><button class="icon-button" aria-label="更多操作" title="更多操作" :aria-expanded="archiveMenuOpen" @click="archiveMenuOpen = !archiveMenuOpen"><MoreHorizontal :size="19" /></button><div v-if="archiveMenuOpen" class="archive-actions-menu"><button @click="openSelectedArchiveSources"><FolderOpen :size="15" />打开来源</button><button @click="openSelectedArchiveStorage"><HardDrive :size="15" />打开资料库</button><button @click="openEditArchive"><Pencil :size="15" />编辑存档</button><button class="danger" @click="selectedArchive && deleteArchive(selectedArchive)"><Trash2 :size="15" />删除存档</button></div></div></div>
        </header>

        <ArchiveMetadata :archive="selectedArchive" :saving-tags="savingTags" @add-tag="addArchiveTag" @remove-tag="removeArchiveTag" />

        <div class="detail-content">
          <section class="timeline-area">
            <div class="section-title"><div><p class="label">版本历史</p><h3>时间节点</h3></div><button class="link-button" @click="showNotice('保留策略将在自动备份阶段接入', 'info')">管理保留策略</button></div>
            <div class="snapshot-create"><input v-model="snapshotDescription" maxlength="160" placeholder="输入新存档描述信息（可留空）" @keydown.enter.prevent="createSnapshotFromDetail" /><button class="accent snapshot-create-button" :class="`is-${snapshotButtonState.state}`" :style="{ '--snapshot-progress': snapshotButtonState.percent / 100 }" :disabled="busyAction !== undefined" @click="createSnapshotFromDetail"><span><Plus :size="16" />{{ busyAction === 'snapshot' ? '创建中' : '创建新快照' }}</span></button></div>
            <div class="timeline-toolbar"><label><Search :size="15" /><input v-model="snapshotSearch" type="search" placeholder="搜索快照描述" /></label><ThemedSelect :model-value="snapshotSort" :options="timelineSortOptions" label="时间线排序" @update:model-value="updateTimelineSort" /></div>
            <div v-if="visibleSnapshots.length" class="timeline-table"><div class="timeline-table-head"><span>备份时间</span><span>描述</span><span>位置 / 大小</span><span>操作</span></div><article v-for="snapshot in visibleSnapshots" :key="snapshot.id" :class="{ selected: selectedSnapshotId === snapshot.id }" tabindex="0" @click="selectedSnapshotId = snapshot.id" @keydown.enter="selectedSnapshotId = snapshot.id"><time>{{ formatTime(snapshot.createdAt) }}</time><span><b>{{ snapshot.title }}</b><small>{{ snapshot.note || snapshotDetail(snapshot) }}</small></span><span>本机 · {{ formatBytes(snapshot.totalBytes) }}</span><div class="timeline-actions"><button class="info" :class="{ active: activityPanelOpen && selectedSnapshotId === snapshot.id }" :aria-label="`查看 ${formatTime(snapshot.createdAt)} 的时间节点详情`" :title="`查看 ${formatTime(snapshot.createdAt)} 的时间节点详情`" :aria-pressed="activityPanelOpen && selectedSnapshotId === snapshot.id" @click.stop="selectedSnapshotId = snapshot.id; activityPanelOpen = true"><Info :size="16" /></button><button :disabled="busyAction !== undefined" :aria-label="`恢复 ${formatTime(snapshot.createdAt)} 的时间节点`" :title="`恢复 ${formatTime(snapshot.createdAt)} 的时间节点`" @click.stop="selectedSnapshotId = snapshot.id; restoreSnapshot()"><RotateCcw :size="16" /><span>恢复</span></button><button :disabled="syncingArchive" :aria-label="`同步 ${formatTime(snapshot.createdAt)} 的时间节点`" :title="`同步 ${formatTime(snapshot.createdAt)} 的时间节点`" @click.stop="selectedSnapshotId = snapshot.id; syncSelectedArchive()"><UploadCloud :size="16" /><span>同步</span></button><button class="danger" :disabled="busyAction !== undefined" :aria-label="`删除 ${formatTime(snapshot.createdAt)} 的时间节点`" :title="`删除 ${formatTime(snapshot.createdAt)} 的时间节点`" @click.stop="deleteSnapshot(snapshot)"><Trash2 :size="16" /></button></div></article></div>
            <div v-else class="timeline-empty"><Clock3 :size="25" /><b>还没有时间节点</b><p>创建首个备份后，可以从这里查看和恢复历史版本。</p><button :disabled="busyAction !== undefined" @click="createSnapshot('初始版本')">创建首个备份</button></div>
          </section>

          <aside class="inspector activity-panel" :class="{ expanded: activityPanelOpen && Boolean(selectedSnapshot) }">
            <template v-if="selectedSnapshot && activityPanelOpen">
              <div class="section-title"><div><p class="label">已选版本</p><h3>{{ formatTime(selectedSnapshot.createdAt) }}</h3></div><span class="verified"><Check :size="13" />完整</span></div>
              <dl><div><dt>类型</dt><dd>{{ selectedSnapshot.title }}</dd></div><div><dt>快照大小</dt><dd>{{ formatBytes(selectedSnapshot.totalBytes) }}</dd></div><div><dt>存储位置</dt><dd>仅本地</dd></div><div><dt>内容校验</dt><dd class="hash">{{ selectedSnapshot.contentHash.slice(0, 6) }}…{{ selectedSnapshot.contentHash.slice(-4) }}</dd></div></dl>
              <div class="changes"><p>内容变化</p><div><span><i class="green"></i>新增</span><b>{{ selectedSnapshot.changes.added }}</b></div><div><span><i class="amber"></i>修改</span><b>{{ selectedSnapshot.changes.modified }}</b></div><div><span><i class="red"></i>删除</span><b>{{ selectedSnapshot.changes.deleted }}</b></div></div>
              <label class="snapshot-note"><span>备注</span><textarea v-model="snapshotNote" maxlength="500" placeholder="记录当前进度、目标或注意事项" @keydown.ctrl.enter.prevent="saveSnapshotNote"></textarea><button :disabled="savingSnapshotNote" @click="saveSnapshotNote"><Save :size="16" />{{ savingSnapshotNote ? '保存中' : '保存备注' }}</button></label>
              <div class="inspector-divider" aria-hidden="true"></div>
              <button class="restore" :disabled="busyAction !== undefined" @click="restoreSnapshot"><RotateCcw :size="16" />{{ busyAction === 'restore' ? '正在恢复' : '恢复到这个时间节点' }}</button><p class="hint">恢复前会先创建当前状态的安全快照。</p>
            </template>
            <div v-else class="inspector-empty"><Clock3 :size="20" /><span>选择时间节点后，点击信息按钮查看详情</span></div>
          </aside>
        </div>
      </section>

      <section v-else class="detail-panel detail-placeholder"><span><FolderArchive :size="31" /></span><h2>本地优先的时间节点管理</h2><p>从左侧添加文件或文件夹，开始保存和恢复历史状态。</p></section>
    </main>

    <div v-if="notice" class="toast" :class="notice.type" role="status"><span>{{ notice.message }}</span><button aria-label="关闭通知" title="关闭通知" @click="notice = undefined"><X :size="15" /></button></div>
    <SettingsDialog v-if="settingsOpen" @close="settingsOpen = false" @saved="handleSettingsChanged" />
    <CloudSettingsDialog v-if="cloudSettingsOpen" @close="cloudSettingsOpen = false" @saved="handleCloudSettingsChanged" />
    <CloudHealthDialog v-if="cloudHealthDialogOpen" :items="cloudHealthCheckItems" :running="cloudHealthCheckRunning" @close="cloudHealthDialogOpen = false" />
    <CreateCategoryDialog
      v-if="categoryDialogOpen"
      :parent-name="categoryRecords.find((category) => category.id === selectedCategoryId)?.name"
      :submitting="creatingCategory"
      :error="createCategoryError"
      @close="categoryDialogOpen = false"
      @submit="createCategory"
    />
    <CreateArchiveDialog
      v-if="createDialogOpen"
      :sources="pendingSources"
      :default-initial-snapshot="appSettings.createInitialSnapshot"
      :picking="pickingSource"
      :submitting="creatingArchive"
      :error="createArchiveError"
      :edit-name="editingArchive?.name"
      :edit-storage-policy="editingArchive?.storagePolicy"
      :edit-sync-mode="editingArchive?.syncMode"
      @close="closeArchiveDialog"
      @pick="pickSources"
      @remove="pendingSources = pendingSources.filter((source) => source.id !== $event)"
      @submit="createArchive"
    />
    <ConfirmDialog v-if="confirmRequest" :title="confirmRequest.title" :message="confirmRequest.message" :confirm-label="confirmRequest.confirmLabel" :destructive="confirmRequest.destructive" @cancel="answerConfirmation(false)" @confirm="answerConfirmation(true)" />
  </div>
</template>
