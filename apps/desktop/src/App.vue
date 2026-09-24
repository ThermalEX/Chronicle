<script setup lang="ts">
import { t, locale, type Locale } from "./services/i18n";
import {
  AlertTriangle, Check, ChevronDown, ChevronLeft, ChevronRight, Clock3, CloudCog, File, Gamepad2, Info,
  Folder, FolderArchive, FolderOpen, HardDrive, LockKeyhole, MoreHorizontal, Moon, Pencil, Plus, RotateCcw, Save,
  RefreshCw, Search, Settings2, SlidersHorizontal, UploadCloud, X,
  Star, Sun, Trash2,
} from "@lucide/vue";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, toRaw, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import CloudCenterDialog from "./components/CloudCenterDialog.vue";
import AppToast from "./components/AppToast.vue";
import CloudHealthDialog from "./components/CloudHealthDialog.vue";
import ArchiveMetadata from "./components/ArchiveMetadata.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import RegistryRestoreDialog from "./components/RegistryRestoreDialog.vue";
import type { RegistryRestoreMode } from "./domain";
import { notifyTrayBackground } from "./services/trayNotification";
import CreateCategoryDialog from "./components/CreateCategoryDialog.vue";
import CreateArchiveDialog from "./components/CreateArchiveDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import SteamScanDialog from "./components/SteamScanDialog.vue";
import UpdateDialog from "./components/UpdateDialog.vue";
import TutorialOverlay from "./components/TutorialOverlay.vue";
import { advanceTutorial, createTutorialState, loadTutorialProgress, saveTutorialProgress, shouldOfferTutorial, tutorialViews, type TutorialEvent, type TutorialProgress, type TutorialState } from "./services/onboarding";
import ThemedSelect, { type ThemedSelectOption } from "./components/ThemedSelect.vue";
import type { ArchiveRecord, ArchiveSource, CategoryRecord, CreateArchiveInput, RepositoryInfo, SnapshotRecord, SourceKind } from "./domain";
import { archiveRepository, isTauriRuntime } from "./services/repository";
import { cloudRepository, syncArchivesAcrossSources, type CloudSyncResult } from "./services/cloud";
import { runCloudHealthCheck, type CloudHealthCheckItem } from "./services/cloudHealthCheck";
import { diagnosticsRepository } from "./services/diagnostics";
import { diagnosticFromError, type DiagnosticContext } from "./services/diagnosticsCore";
import { compareArchiveNames, type ArchiveSortMode } from "./services/archiveSorting";
import { selectArchivePanelCategory, selectCategoryPanel } from "./services/archiveWorkspace";
import { applyAppearance, normalizeAppearance, type Appearance } from "./services/appearance";
import { floatingMenuStyle, positionFloatingMenu } from "./services/floatingMenu";
import { filterTimeline, type TimelineSort } from "./services/snapshotTimeline";
import { appSettings, cloudLibraryIndicator, cloudSettings, enabledCloudSources, initializeSettings, saveAppSettings, shortcutMatches, type CloudHealth } from "./services/settings";
import { runAcrossEnabledSources, type SourceSyncOutcome } from "./services/multiSourceSync";
import { categoryBreadcrumb } from "./services/categoryBreadcrumb";
import { formatCurrentTime, millisecondsUntilNextMinute } from "./services/currentTime";
import { appMetadata } from "./services/appMetadata";
import { checkForUpdate, type ReleaseUpdate } from "./services/updateService";

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
const timelineSortOptions = computed<ThemedSelectOption[]>(() => [
  { value: "newest", label: t("时间 新–旧") },
  { value: "oldest", label: t("时间 旧–新") },
]);
const searchTerm = ref("");
const searchInput = ref<HTMLInputElement>();
const loading = ref(true);
const refreshingLibrary = ref(false);
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
const steamScanOpen = ref(false);
const cloudSettingsOpen = ref(false);
const syncingArchive = ref(false);
const syncingAllArchives = ref(false);
const syncProgress = ref<{ current: number; total: number }>();
const syncProgressTarget = ref<"current" | "all">();
const busyAction = ref<"snapshot" | "restore">();
const savingTags = ref(false);
const savingSnapshotNote = ref(false);
const lockingSnapshotId = ref<string>();
const registryRestoreRequest = ref<{ archive: ArchiveRecord; snapshot: SnapshotRecord }>();
const repositoryInfo = ref<RepositoryInfo>({ path: "", totalBytes: 0 });
const editingArchive = ref<ArchiveRecord>();
const highlightSources = ref(false);
const archiveMenuOpen = ref(false);
const trashDropActive = ref(false);
const treeMenu = ref<{ kind: "archive" | "category"; id: string }>();
const treeMenuAnchor = ref<DOMRect>();
const treeMenuStyle = computed(() => treeMenuAnchor.value
  ? floatingMenuStyle(positionFloatingMenu(treeMenuAnchor.value, { width: 168, height: 96 }, { width: window.innerWidth, height: window.innerHeight }))
  : undefined);
const confirmRequest = ref<{ title: string; message: string; confirmLabel: string; destructive: boolean }>();
const closeRequestOpen = ref(false);
const updateChecking = ref(false);
const availableUpdate = ref<ReleaseUpdate>();
const tutorial = ref<TutorialState>({ step: "inactive", route: "local" });
const tutorialProgress = ref<TutorialProgress>({ status: "legacy", seenTips: [] });
const tutorialActive = computed(() => tutorial.value.step !== "inactive");
const tutorialAppearanceSaving = ref(false);
const tutorialLanguageSaving = ref(false);
async function changeTutorialLanguage(language: Locale) {
  if (tutorialLanguageSaving.value) return;
  tutorialLanguageSaving.value = true;
  try { await saveAppSettings({ ...appSettings, language }); }
  catch (error) { showNotice(t("语言未能保存：{error}", { error: readableError(error) }), "error"); }
  finally { tutorialLanguageSaving.value = false; }
}
async function changeTutorialAppearance(appearance: Appearance) {
  if (tutorialAppearanceSaving.value) return;
  const previous = normalizeAppearance(appSettings);
  tutorialAppearanceSaving.value = true;
  applyAppearance(appearance);
  try { await saveAppSettings({ ...appSettings, ...appearance }); }
  catch (error) {
    Object.assign(appSettings, previous);
    applyAppearance(previous);
    showNotice(t("主题未能保存：{value}", { value: readableError(error) }), "error");
  } finally { tutorialAppearanceSaving.value = false; }
}
let tutorialSave = Promise.resolve();
let startupUpdatePending = false;
function persistTutorial(status: TutorialProgress["status"]) {
  tutorialProgress.value = { ...tutorialProgress.value, status };
  const progress = { ...tutorialProgress.value };
  tutorialSave = tutorialSave.then(() => saveTutorialProgress(progress)).catch(() => {
    showNotice(t("教程状态未能保存，下次启动可能再次显示。"), "error");
  });
}
function tutorialEvent(event: TutorialEvent) {
  const previous = tutorial.value.step;
  tutorial.value = advanceTutorial(tutorial.value, event);
  if (event.type === "skip") persistTutorial("skipped");
  else if (previous === "finish" && tutorial.value.step === "inactive") persistTutorial("completed");
}
function nextTutorial() {
  if (tutorial.value.step === "name") {
    const input = document.querySelector<HTMLInputElement>('[data-tour="name"] input');
    if (!input?.value.trim()) { input?.focus(); showNotice(t("请先填写存档名称。"), "info"); return; }
    tutorialEvent({ type: "name-ready" });
  } else tutorialEvent({ type: "next" });
}
function rememberTutorialTip(tip: TutorialProgress["seenTips"][number]) {
  if (tutorialProgress.value.seenTips.includes(tip)) return;
  tutorialProgress.value = { ...tutorialProgress.value, seenTips: [...tutorialProgress.value.seenTips, tip] };
  persistTutorial(tutorialProgress.value.status);
}
function localTutorial() {
  tutorialEvent({ type: "choose-local" });
  cloudSettingsOpen.value = false;
}
function restartTutorial() {
  settingsOpen.value = false;
  archivePanelCollapsed.value = false;
  snapshotSearch.value = "";
  snapshotSort.value = "newest";
  tutorial.value = createTutorialState();
  persistTutorial("pending");
}
function useExistingTutorial() {
  if (!selectedArchive.value) return;
  tutorialEvent({ type: "use-existing", archiveId: selectedArchive.value.id, hasSnapshot: snapshots.value.length > 0 });
}
watch(cloudSettingsOpen, (open) => tutorialEvent({ type: open ? "cloud-opened" : "dialog-closed" }));
watch(steamScanOpen, (open) => tutorialEvent({ type: open ? "steam-opened" : "steam-closed" }));
watch(createDialogOpen, (open) => {
  if (open && !editingArchive.value) tutorialEvent({ type: "create-opened" });
  else if (!open && !creatingArchive.value) tutorialEvent({ type: "dialog-closed" });
});
watch(() => pendingSources.value.length, (count) => { if (count > 0) tutorialEvent({ type: "sources-ready" }); });
watch(tutorialActive, (active) => {
  if (!active && startupUpdatePending) { startupUpdatePending = false; void checkForApplicationUpdate(); }
});
let confirmResolver: ((confirmed: boolean) => void) | undefined;
const notice = ref<{ type: "success" | "error" | "info"; message: string }>();
let noticeTimer: number | undefined;
const cloudHealth = ref<CloudHealth>({ status: "unchecked" });
const cloudHealthDialogOpen = ref(false);
const cloudHealthCheckRunning = ref(false);
const cloudHealthCheckItems = ref<CloudHealthCheckItem[]>([]);
const currentTime = ref(formatCurrentTime(new Date()));
watch(locale, () => { currentTime.value = formatCurrentTime(new Date()); });
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

async function hideMainWindowToTray(): Promise<void> {
  closeRequestOpen.value = false;
  await invoke("hide_main_window");
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
  const items = await runCloudHealthCheck(cloudSettings.sources, (source) => cloudRepository.test(source, ""), (next) => { cloudHealthCheckItems.value = next; });
  cloudHealthCheckRunning.value = false;
  const failed = items.find((item) => item.status === "failed");
  cloudHealth.value = failed ? { status: "unavailable", sourceName: failed.name, reason: failed.reason } : { status: "available" };
}

function openCloudHealthDialog(): void {
  if (!cloudHealthCheckRunning.value) void checkCloudSources(true);
  else cloudHealthDialogOpen.value = true;
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
  return [{ nodeType: "category", id: "all", name: t("全部存档"), depth: 0, count: archives.value.length, icon: FolderArchive, hasChildren: false }, ...rows];
});
const selectedCategoryName = computed(() => categoryRecords.value.find((category) => category.id === selectedCategoryId.value)?.name ?? t("全部存档"));
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
const syncProgressFraction = computed(() => syncProgress.value ? syncProgress.value.current / syncProgress.value.total : 0);
const currentSyncProgressFraction = computed(() => syncProgressTarget.value === "current" ? syncProgressFraction.value : 0);
const allSyncProgressFraction = computed(() => syncProgressTarget.value === "all" ? syncProgressFraction.value : 0);
const syncProgressText = computed(() => syncProgress.value ? `${syncProgress.value.current} / ${syncProgress.value.total}` : "");

function showNotice(message: string, type: "success" | "error" | "info" = "success") {
  notice.value = { message, type };
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => { notice.value = undefined; }, 3200);
}

async function checkForApplicationUpdate(manual = false, channel = appSettings.updateChannel): Promise<void> {
  if (updateChecking.value) return;
  updateChecking.value = true;
  try {
    const update = await checkForUpdate(channel, appMetadata.version);
    if (update) availableUpdate.value = update;
    else if (manual) showNotice(t("当前已是最新版本"), "info");
  } catch (error) {
    if (manual) showNotice(error instanceof Error ? error.message : t("检查更新失败"), "error");
  } finally {
    updateChecking.value = false;
  }
}

function readableError(error: unknown): string {
  return diagnosticFromError(error, { operation: t("应用操作") }).message;
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
  if (!timestamp) return t("尚未备份");
  const date = new Date(timestamp);
  const today = new Date();
  if (date.toDateString() === today.toDateString()) {
    return t("今天 {time}", { time: date.toLocaleTimeString(locale.value, { hour: "2-digit", minute: "2-digit", hour12: false }) });
  }
  return date.toLocaleString(locale.value, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false });
}

function displaySourcePath(path: string): string {
  return path.split(" · ").map((part) => part.startsWith("\\\\?\\") ? part.slice(4) : part).join(" · ");
}

function archiveNeedsLocation(archive: ArchiveRecord): boolean {
  return archive.sources.some((source) => !source.path);
}

function snapshotDetail(snapshot: SnapshotRecord): string {
  const { added, modified, deleted } = snapshot.changes;
  if (!added && !modified && !deleted) return t("内容与上一个时间节点一致");
  return [added ? t("新增 {added}", { added }) : "", modified ? t("修改 {modified}", { modified }) : "", deleted ? t("删除 {deleted}", { deleted }) : ""].filter(Boolean).join(" · ");
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

async function refreshLibrary(): Promise<void> {
  if (refreshingLibrary.value) return;
  refreshingLibrary.value = true;
  try { await handleCloudDownload(); }
  catch (error) { reportError(error, { operation: t("刷新资料库") }); }
  finally { refreshingLibrary.value = false; }
}

async function handleSettingsChanged() {
  applyAppearance(appSettings);
  if (isTauriRuntime) await archiveRepository.refreshAutoBackup();
  await Promise.all([refreshArchives(), refreshCategories(), refreshRepositoryInfo()]);
  showNotice(t("设置已保存"));
}

function handleCloudSettingsChanged(connectionVerified = false): void {
  if (tutorial.value.step === "cloud-form" && connectionVerified) {
    tutorialEvent({ type: "cloud-saved" });
    cloudSettingsOpen.value = false;
  }
  cloudHealth.value = { status: "unchecked" };
  cloudHealthCheckItems.value = [];
  showNotice(t("云端设置已保存"));
}

async function handleCloudDownload(): Promise<void> {
  await Promise.all([refreshArchives(selectedArchiveId.value), refreshCategories(), refreshRepositoryInfo()]);
  if (selectedArchive.value) await refreshSnapshots(selectedArchive.value.id);
  showNotice(t("云端存档已下载，资料库已刷新"));
}

async function handleCloudSettingsDownload(): Promise<void> {
  await initializeSettings();
  applyAppearance(appSettings);
  await handleCloudDownload();
  showNotice(t("云端应用设置已应用"));
}

async function openRepositoryFolder() {
  try {
    await archiveRepository.openRepositoryFolder();
  } catch (error) {
    reportError(error, { operation: t("打开资料库") });
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
    reportError(error, { operation: t("打开存档来源"), archiveId: archive.id });
  }
}

async function refreshSnapshots(archiveId?: string) {
  snapshots.value = archiveId ? await archiveRepository.listSnapshots(archiveId) : [];
  selectedSnapshotId.value = snapshots.value[0]?.id;
}

function openCreateArchive() {
  editingArchive.value = undefined;
  highlightSources.value = false;
  pendingSources.value = [];
  createArchiveError.value = undefined;
  createDialogOpen.value = true;
}

function openEditArchive() {
  if (!selectedArchive.value) return;
  openArchiveEditor(selectedArchive.value);
}

function openArchiveEditor(archive: ArchiveRecord, highlightSourceSelection = false) {
  selectArchive(archive.id);
  editingArchive.value = archive;
  highlightSources.value = highlightSourceSelection;
  pendingSources.value = archive.sources.map((source) => ({ ...source }));
  createArchiveError.value = undefined;
  archiveMenuOpen.value = false;
  createDialogOpen.value = true;
}

function closeArchiveDialog() {
  createDialogOpen.value = false;
  editingArchive.value = undefined;
  highlightSources.value = false;
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

async function pickSources(kind: Exclude<SourceKind, "registry">) {
  if (!isTauriRuntime && (!("showOpenFilePicker" in window) || !("showDirectoryPicker" in window))) {
    showNotice(t("当前环境不支持本地文件系统访问，请使用 Edge 或桌面版 Chronicle"), "error");
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
  if (["sources", "name", "storage", "automation"].includes(tutorial.value.step)) {
    showNotice(t("请先阅读当前提示，完成保存方式与自动化介绍后再创建。"), "info");
    return;
  }
  creatingArchive.value = true;
  createArchiveError.value = undefined;
  try {
    if (editingArchive.value) {
      const archiveId = editingArchive.value.id;
      await archiveRepository.updateArchive(archiveId, input);
      await archiveRepository.setArchiveAutomation(archiveId, input.autoBackupEnabled, input.automaticUploadEnabled);
      await archiveRepository.refreshAutoBackup();
      closeArchiveDialog();
      await refreshArchives(archiveId);
      await refreshRepositoryInfo();
      showNotice(t("存档设置已更新"));
      return;
    }
    const categoryId = selectedCategoryId.value === "all" ? undefined : selectedCategoryId.value;
    const archive = await archiveRepository.createArchive({ ...input, categoryId });
    await archiveRepository.setArchiveAutomation(archive.id, input.autoBackupEnabled, input.automaticUploadEnabled);
    await archiveRepository.refreshAutoBackup();
    createDialogOpen.value = false;
    if (categoryId) expandedCategoryIds.value = new Set([...expandedCategoryIds.value, categoryId]);
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    await refreshRepositoryInfo();
    if (input.createInitialSnapshot) {
      tutorialEvent({ type: "archive-created", archiveId: archive.id });
      await createSnapshot(t("初始版本"));
    } else {
      tutorialEvent({ type: "archive-created", archiveId: archive.id });
      showNotice(t("已创建存档“{name}”", { name: archive.name }));
    }
  } catch (error) {
    createArchiveError.value = readableError(error);
  } finally {
    creatingArchive.value = false;
  }
}

async function deleteArchive(archive: ArchiveRecord) {
  archiveMenuOpen.value = false;
  const action = appSettings.recycleBinEnabled ? t("移入回收站") : t("永久删除");
  const confirmed = await requestConfirmation(
    `${action}“${archive.name}”`,
    appSettings.recycleBinEnabled ? t("存档及其全部时间节点将移入回收站。") : t("存档及其全部时间节点将被永久删除，无法恢复。"),
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
    showNotice(appSettings.recycleBinEnabled ? t("存档已移入回收站") : t("存档已永久删除"));
  } catch (error) {
    showNotice(readableError(error), "error");
  }
}

async function deleteCategory(categoryId: string) {
  const category = categoryRecords.value.find((item) => item.id === categoryId);
  if (!category) return;
  const ids = descendantIds(categoryId);
  const archiveCount = archives.value.filter((archive) => archive.categoryId && ids.has(archive.categoryId)).length;
  const action = appSettings.recycleBinEnabled ? t("移入回收站") : t("永久删除");
  const confirmed = await requestConfirmation(
    t("{action}分类“{name}”", { action: action, name: category.name }),
    t("该分类的子分类和 {archiveCount} 个存档将一并{value}。", { archiveCount: archiveCount, value: appSettings.recycleBinEnabled ? t("移入回收站") : t("永久删除") }),
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
    showNotice(appSettings.recycleBinEnabled ? t("分类已移入回收站") : t("分类已永久删除"));
  } catch (error) {
    showNotice(readableError(error), "error");
  }
}

function archiveMoveOptions(): ThemedSelectOption[] {
  return [
    { value: null, label: t("根目录") },
    ...categoryRecords.value.map((category) => ({ value: category.id, label: category.name })),
  ];
}

function categoryMoveOptions(categoryId: string): ThemedSelectOption[] {
  const excluded = descendantIds(categoryId);
  return [
    { value: null, label: t("根目录") },
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
  const closeOutside = (event: PointerEvent) => {
    const target = event.target;
    if (!(target instanceof Element) || target.closest(".tree-menu, .tree-more")) return;
    // The move selector renders its choices in a separate Teleport under body.
    const popup = target.closest('[role="listbox"]');
    const selector = document.querySelector('.tree-menu [aria-haspopup="listbox"]');
    if (popup && selector && popup.getAttribute("aria-label") === selector.getAttribute("aria-label")) return;
    close();
  };
  const closeOnEscape = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
  document.addEventListener("pointerdown", closeOutside, true);
  document.addEventListener("keydown", closeOnEscape, true);
  window.addEventListener("scroll", close, true);
  window.addEventListener("resize", close);
  onCleanup(() => {
    document.removeEventListener("pointerdown", closeOutside, true);
    document.removeEventListener("keydown", closeOnEscape, true);
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

async function createSnapshot(title = t("手动备份")) {
  if (!selectedArchive.value || busyAction.value) return;
  busyAction.value = "snapshot";
  try {
    const snapshot = await archiveRepository.createSnapshot(toRaw(selectedArchive.value), title, false);
    await refreshArchives(selectedArchive.value.id);
    await refreshSnapshots(selectedArchive.value.id);
    await refreshRepositoryInfo();
    selectedSnapshotId.value = snapshot.id;
    tutorialEvent({ type: "snapshot-created", archiveId: selectedArchive.value.id });
    await uploadNewSnapshot(selectedArchive.value);
    showNotice(t("时间节点已创建，保存 {length} 个文件", { length: snapshot.files.length }));
  } catch (error) {
    reportError(error, { operation: t("创建备份"), archiveId: selectedArchive.value?.id });
  } finally {
    busyAction.value = undefined;
  }
}

async function uploadNewSnapshot(archive: ArchiveRecord): Promise<void> {
  if (!archive.automaticUploadEnabled || archive.storagePolicy !== "local_and_remote") return;
  const outcomes = await syncArchiveToSources(archive, enabledCloudSources(cloudSettings));
  reportSourceFailures(outcomes, archive, t("自动上传"));
}

function syncArchiveToSources(archive: ArchiveRecord, sources: ReturnType<typeof enabledCloudSources>) {
  return runAcrossEnabledSources(
    sources,
    async (source) => {
      const result = await cloudRepository.sync(source.id, archive.id);
      if (result.status !== "conflict") await cloudRepository.uploadSyncMetadata(source.id, [archive.id]);
      return result;
    },
    () => { if (syncProgress.value) syncProgress.value.current += 1; },
  );
}

async function syncSelectedArchive() {
  const archive = selectedArchive.value;
  if (!archive || syncingArchive.value || syncingAllArchives.value) return;
  if (!isTauriRuntime) {
    showNotice(t("云同步仅在 Chronicle 桌面端可用"), "error");
    return;
  }
  if (archive.storagePolicy !== "local_and_remote") {
    showNotice(t("当前存档仅使用本地存储，请先在编辑存档中启用云端保存"), "info");
    return;
  }
  const sources = enabledCloudSources(cloudSettings);
  if (!sources.length) {
    cloudSettingsOpen.value = true;
    showNotice(t("请先添加同步源，并点击开始同步"), "info");
    return;
  }
  syncingArchive.value = true;
  syncProgressTarget.value = "current";
  syncProgress.value = { current: 0, total: sources.length };
  try {
    const outcomes = await syncArchiveToSources(archive, sources);
    reportSourceFailures(outcomes, archive, t("同步存档"));
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    const completed = outcomes.filter((outcome): outcome is Extract<SourceSyncOutcome<CloudSyncResult>, { status: "fulfilled" }> => outcome.status === "fulfilled");
    if (completed.length) {
      const hasConflict = completed.some((outcome) => outcome.value.status === "conflict");
      showNotice(t("已完成 {length} / {length2} 个同步源", { length: completed.length, length2: outcomes.length }), hasConflict ? "info" : "success");
    }
  } catch (error) {
    reportError(error, { operation: t("同步存档"), archiveId: archive.id });
  } finally {
    syncingArchive.value = false;
    syncProgress.value = undefined;
    syncProgressTarget.value = undefined;
  }
}

async function syncAllArchives() {
  if (syncingArchive.value || syncingAllArchives.value) return;
  if (!isTauriRuntime) {
    showNotice(t("云同步仅在 Chronicle 桌面端可用"), "error");
    return;
  }
  const sources = enabledCloudSources(cloudSettings);
  if (!sources.length) {
    cloudSettingsOpen.value = true;
    showNotice(t("请先添加同步源，并点击开始同步"), "info");
    return;
  }
  const remoteArchives = archives.value.filter((archive) => archive.storagePolicy === "local_and_remote");
  if (!remoteArchives.length) {
    showNotice(t("没有启用云端保存的存档"), "info");
    return;
  }
  syncingAllArchives.value = true;
  syncProgressTarget.value = "all";
  syncProgress.value = { current: 0, total: (remoteArchives.length + 1) * sources.length };
  try {
    const { outcomes, metadataOutcomes, hasFailures } = await syncArchivesAcrossSources(sources, remoteArchives, () => {
      if (syncProgress.value) syncProgress.value.current += 1;
    });
    for (const outcome of outcomes) reportSourceFailures([outcome], outcome.item, t("同步全部存档"));
    const succeeded = outcomes.filter((outcome): outcome is Extract<(typeof outcomes)[number], { status: "fulfilled" }> => outcome.status === "fulfilled");
    const completed = succeeded.length;
    const conflicts = succeeded.some((outcome) => outcome.value.status === "conflict");
    for (const outcome of metadataOutcomes) {
      if (outcome.status === "rejected") reportError(outcome.reason, { operation: t("同步云端元数据"), sourceId: outcome.source.id });
    }
    await refreshArchives(selectedArchiveId.value);
    if (selectedArchive.value) await refreshSnapshots(selectedArchive.value.id);
    showNotice(t("已完成 {completed} / {value} 项存档同步{value3}", { completed: completed, value: remoteArchives.length * sources.length, value3: hasFailures ? t("，部分操作失败，请查看错误记录") : "" }), conflicts || hasFailures ? "info" : "success");
  } catch (error) {
    reportError(error, { operation: t("同步全部存档") });
  } finally {
    syncingAllArchives.value = false;
    syncProgress.value = undefined;
    syncProgressTarget.value = undefined;
  }
}

async function restoreSnapshot() {
  const archive = selectedArchive.value;
  const snapshot = selectedSnapshot.value;
  if (!archive || !snapshot || busyAction.value) return;
  if (archive.sources.some((source) => source.kind === "registry")) {
    registryRestoreRequest.value = { archive, snapshot };
    return;
  }
  const confirmed = window.confirm(t("将“{name}”恢复到 {time}。Chronicle 会先保存当前状态，是否继续？", { name: archive.name, time: formatTime(snapshot.createdAt) }));
  if (!confirmed) return;
  await executeRestore(archive, snapshot);
}

async function confirmRegistryRestore(mode: RegistryRestoreMode) {
  const request = registryRestoreRequest.value;
  registryRestoreRequest.value = undefined;
  if (request) await executeRestore(request.archive, request.snapshot, mode);
}

async function executeRestore(archive: ArchiveRecord, snapshot: SnapshotRecord, mode?: RegistryRestoreMode) {
  if (busyAction.value) return;
  busyAction.value = "restore";
  try {
    await archiveRepository.restoreSnapshot(archive, snapshot, mode);
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    showNotice(t("恢复完成，原状态已保存为安全快照"));
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
    showNotice(t("快照备注已保存"));
  } catch (error) {
    reportError(error, { operation: t("保存快照备注"), archiveId: archive.id });
  } finally {
    savingSnapshotNote.value = false;
  }
}

async function deleteSnapshot(snapshot: SnapshotRecord): Promise<void> {
  const archive = selectedArchive.value;
  if (!archive || busyAction.value || snapshot.locked) return;
  const confirmed = await requestConfirmation(
    t("永久删除时间节点"),
    t("将永久删除 {time} 的快照文件及其备注，无法恢复。", { time: formatTime(snapshot.createdAt) }),
    t("永久删除"),
    true,
  );
  if (!confirmed) return;
  try {
    await archiveRepository.deleteSnapshot(archive.id, snapshot.id);
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    await refreshRepositoryInfo();
    showNotice(t("时间节点已永久删除"));
  } catch (error) {
    reportError(error, { operation: t("删除时间节点"), archiveId: archive.id });
  }
}

async function toggleSnapshotLock(snapshot: SnapshotRecord): Promise<void> {
  const archive = selectedArchive.value;
  if (!archive || lockingSnapshotId.value) return;
  lockingSnapshotId.value = snapshot.id;
  try {
    const updated = await archiveRepository.setSnapshotLocked(archive.id, snapshot.id, !snapshot.locked);
    snapshots.value = snapshots.value.map((item) => item.id === updated.id ? updated : item);
    tutorialEvent({ type: "locked" });
    showNotice(updated.locked ? t("已标记为重要快照，不会自动清理；删除前需先解锁") : t("快照已解锁"));
  } catch (error) { reportError(error, { operation: t("更新快照锁定状态"), archiveId: archive.id }); }
  finally { lockingSnapshotId.value = undefined; }
}

function addRegistrySource(path: string): void {
  if (!isTauriRuntime) { createArchiveError.value = t("注册表来源仅支持 Windows 桌面版 Chronicle"); return; }
  if (pendingSources.value.some((source) => source.kind === "registry" && source.path.toLowerCase() === path.toLowerCase())) return;
  pendingSources.value.push({ id: crypto.randomUUID(), name: path.split("\\").at(-1) || path, path, kind: "registry" });
  createArchiveError.value = undefined;
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
  const title = snapshotDescription.value.trim() || t("手动备份");
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
    showNotice(name ? t("存档已移入“{name}”", { name: name }) : t("存档已移至根目录"));
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
    showNotice(t("标签已更新"));
  } catch (error) {
    showNotice(readableError(error), "error");
  } finally {
    savingTags.value = false;
  }
}

function addArchiveTag(tag: string) {
  const tags = selectedArchive.value?.tags ?? [];
  if (tags.length >= 20) {
    showNotice(t("每个存档最多添加 20 个标签"), "error");
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
    const movedName = categoryRecords.value.find((category) => category.id === categoryIdToMove)?.name ?? t("分类");
    try {
      await archiveRepository.moveCategory(categoryIdToMove, parentId);
      if (parentId) expandedCategoryIds.value = new Set([...expandedCategoryIds.value, parentId]);
      await refreshCategories();
      showNotice(parentId ? t("“{movedName}”已移入目标分类", { movedName: movedName }) : t("“{movedName}”已移至根目录", { movedName: movedName }));
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
  if (steamScanOpen.value) return;
  if (tutorialActive.value && !confirmRequest.value && !closeRequestOpen.value) return;
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
    startupUpdatePending = appSettings.checkForUpdates;
    if (isTauriRuntime) {
      await archiveRepository.refreshAutoBackup();
      await listen<{ archiveId: string; error?: string }>("auto-backup-created", async ({ payload }) => {
        await Promise.all([refreshArchives(payload.archiveId), refreshSnapshots(payload.archiveId), refreshRepositoryInfo()]);
        const archive = archives.value.find((item) => item.id === payload.archiveId);
        if (archive) await uploadNewSnapshot(archive);
        showNotice(archive ? t("“{name}”已自动备份", { name: archive.name }) : t("已自动备份"));
      });
      await listen<{ archiveId: string; error?: string }>("auto-backup-failed", ({ payload }) => reportError(payload.error ?? t("自动备份失败"), { operation: t("自动备份"), archiveId: payload.archiveId }));
      await listen("chronicle-close-requested", () => { closeRequestOpen.value = true; });
      await listen("chronicle-hidden-to-tray", () => { void notifyTrayBackground(appSettings.notifications); });
    }
    await refreshCategories();
    await refreshArchives();
    await refreshRepositoryInfo();
    if (appSettings.checkCloudOnLaunch && isTauriRuntime) void checkCloudSources();
    try {
      tutorialProgress.value = await loadTutorialProgress(archives.value.length > 0 || categoryRecords.value.length > 0);
      if (shouldOfferTutorial(tutorialProgress.value)) tutorial.value = createTutorialState();
    } catch { /* A damaged local tutorial marker must not block the application. */ }
    if (!tutorialActive.value && startupUpdatePending) { startupUpdatePending = false; void checkForApplicationUpdate(); }
  }
  catch (error) { showNotice(readableError(error), "error"); }
  finally { loading.value = false; }
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleShortcut);
  window.clearTimeout(noticeTimer);
  window.clearTimeout(clockTimer);
});
</script>

<template>
  <div class="app-shell">
    <header class="titlebar">
      <div class="brand"><time :datetime="currentTime">{{ currentTime }}</time></div>
      <div class="sync-states"><button class="sync-state sync-state-button" :title="t('打开本地资料库')" @click="openRepositoryFolder"><i></i>{{ t("本地资料库可用") }}</button><button class="sync-state sync-state-button" :class="cloudStateClass" :title="cloudHealth.reason || t('检测云端资料库')" @click="openCloudHealthDialog"><i :class="{ pulse: cloudHealth.status === 'checking' }"></i>{{ cloudLibrary.label }}</button><button class="sync-state sync-state-button sync-progress-button" :style="{ '--sync-progress': allSyncProgressFraction }" :disabled="syncingArchive || syncingAllArchives" :title="t('同步所有启用云端保存的存档')" @click="syncAllArchives"><span><UploadCloud :size="16" />{{ syncingAllArchives ? t("正在同步 {syncProgressText}", { syncProgressText: syncProgressText }) : t("同步所有存档") }}</span></button></div>
      <div class="toolbar"><button class="toolbar-action" data-tour="steam-entry" :aria-label="t('游戏存档识别')" :title="t('游戏存档识别')" @click="steamScanOpen = true"><Gamepad2 :size="19" /></button><button class="toolbar-action" data-tour="cloud-entry" :aria-label="t('云端设置')" :title="t('云端设置')" @click="cloudSettingsOpen = true"><CloudCog :size="17" /></button><button class="toolbar-action" :aria-label="t('应用设置')" :title="t('应用设置')" @click="settingsOpen = true"><Settings2 :size="17" /></button></div>
    </header>

    <aside class="sidebar">
      <div class="add-control"><button data-tour="create-entry" class="add-button" @click="openCreateArchive"><Plus :size="18" />{{ t("添加存档") }}</button></div>
      <nav :aria-label="t('存档分类')">
        <div class="nav-heading"><p class="label">{{ t("资料库") }}</p><span><button :disabled="refreshingLibrary" :aria-label="t('刷新资料库')" :title="t('刷新资料库')" @click="refreshLibrary"><RefreshCw :size="15" /></button><button :aria-label="t('添加分类')" :title="t('添加分类')" @click="openCategoryDialog"><Plus :size="15" /></button></span></div>
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
            <button v-if="node.id !== 'all'" class="disclosure" :class="{ hidden: !node.hasChildren }" :aria-label="t('{value} {name}', { value: expandedCategoryIds.has(node.id) ? t('折叠') : t('展开'), name: node.name })" :title="t('{value} {name}', { value: expandedCategoryIds.has(node.id) ? t('折叠') : t('展开'), name: node.name })" :aria-expanded="node.hasChildren ? expandedCategoryIds.has(node.id) : undefined" @click="toggleCategory(node.id)"><ChevronDown v-if="expandedCategoryIds.has(node.id)" :size="14" /><ChevronRight v-else :size="14" /></button>
            <span v-else class="disclosure-spacer"></span>
            <button class="category-select" :draggable="node.id !== 'all'" :title="node.id === 'all' ? t('将分类或存档拖到这里可移至根目录') : undefined" @dragstart.stop="node.id !== 'all' && startCategoryDrag(node.id, $event)" @click="selectCategory(node.id)"><component :is="node.icon" :size="17" /><span>{{ node.name }}</span><span class="category-suffix"><LockKeyhole v-if="node.id === 'all'" :size="12" :aria-label="t('固定根目录')" /><small>{{ node.count }}</small></span></button>
            <div v-if="node.id !== 'all'" class="tree-more"><button :aria-label="t('分类操作')" :title="t('分类操作')" :aria-expanded="treeMenu?.kind === 'category' && treeMenu.id === node.id" @click="toggleTreeMenu('category', node.id, $event)"><MoreHorizontal :size="14" /></button><Teleport to="body"><div v-if="treeMenu?.kind === 'category' && treeMenu.id === node.id" class="tree-menu" :style="treeMenuStyle"><label>{{ t("移动到") }}<ThemedSelect :model-value="node.parentId ?? null" :options="categoryMoveOptions(node.id)" :label="t('移动分类 {name}', { name: node.name })" @update:model-value="moveCategoryFromMenu(node.id, $event)" /></label><button class="danger" @click="deleteCategory(node.id)"><Trash2 :size="13" />{{ t("删除分类") }}</button></div></Teleport></div>
          </div>
          <div v-else class="archive-tree-row" :class="{ active: activeTreeNodeId === `archive:${node.id}`, 'needs-location': archiveNeedsLocation(node.archive) }" :style="{ paddingLeft: `${4 + node.depth * 16}px` }" @dragend="finishDrag">
            <span class="disclosure-spacer"></span>
            <button class="archive-tree-select" draggable="true" :title="archiveNeedsLocation(node.archive) ? t('{name}：等待定位本机来源', { name: node.archive.name }) : node.archive.name" @dragstart.stop="startArchiveDrag(node.id, $event)" @click="selectArchiveFromTree(node.archive)"><AlertTriangle v-if="archiveNeedsLocation(node.archive)" :size="16" /><File v-else :size="16" /><span>{{ node.archive.name }}</span></button>
            <div class="tree-more"><button :aria-label="t('存档移动操作')" :title="t('存档移动操作')" :aria-expanded="treeMenu?.kind === 'archive' && treeMenu.id === node.id" @click="toggleTreeMenu('archive', node.id, $event)"><MoreHorizontal :size="14" /></button><Teleport to="body"><div v-if="treeMenu?.kind === 'archive' && treeMenu.id === node.id" class="tree-menu" :style="treeMenuStyle"><label>{{ t("移动到") }}<ThemedSelect :model-value="node.archive.categoryId ?? null" :options="archiveMoveOptions()" :label="t('移动存档 {name}', { name: node.archive.name })" @update:model-value="moveArchiveFromMenu(node.id, $event)" /></label><button class="danger" @click="deleteArchive(node.archive)"><Trash2 :size="13" />{{ t("删除存档") }}</button></div></Teleport></div>
          </div>
        </template>
      </nav>
      <div class="spacer"></div>
      <div v-if="draggedArchiveId || draggedCategoryId" class="trash-drop-zone" :class="{ active: trashDropActive }" @dragover="handleTrashDragOver" @dragleave="trashDropActive = false" @drop.prevent="dropInTrash"><Trash2 :size="19" /><span><b>{{ appSettings.recycleBinEnabled ? t("移入回收站") : t("永久删除") }}</b><small>{{ t("拖到这里后松开") }}</small></span></div>
      <section class="storage"><div><HardDrive :size="17" /><span>{{ t("本地存储") }}</span><b>{{ formatBytes(repositoryInfo.totalBytes) }}</b><button :aria-label="t('打开本地资料库文件夹')" :title="t('打开本地资料库文件夹')" @click="openRepositoryFolder"><FolderOpen :size="14" /></button></div><small :title="repositoryInfo.path">{{ repositoryInfo.path || t("Chronicle 本地资料库") }}</small></section>
      <div class="account"><span class="avatar">T</span><span><b>ThermalEX</b><small>{{ t("本机设备") }}</small></span></div>
    </aside>

    <main class="workspace" :class="{ 'archive-panel-collapsed': archivePanelCollapsed }">
      <section v-show="!archivePanelCollapsed" class="archive-panel" aria-labelledby="archives-title">
        <div class="panel-title"><div><h1 id="archives-title">{{ selectedCategoryName }}</h1><p class="category-path">{{ selectedCategoryPath }}</p></div><div class="sort-control"><button class="icon-button" :aria-label="t('排列方式')" :title="t('排列方式')" :aria-expanded="sortMenuOpen" @click="sortMenuOpen = !sortMenuOpen"><SlidersHorizontal :size="18" /></button><div v-if="sortMenuOpen" class="sort-menu"><button :class="{ active: sortMode === 'newest' }" @click="sortMode = 'newest'; sortMenuOpen = false">{{ t("时间 新–旧") }}</button><button :class="{ active: sortMode === 'oldest' }" @click="sortMode = 'oldest'; sortMenuOpen = false">{{ t("时间 旧–新") }}</button><button :class="{ active: sortMode === 'nameAsc' }" @click="sortMode = 'nameAsc'; sortMenuOpen = false">{{ t("名称 A–Z") }}</button><button :class="{ active: sortMode === 'nameDesc' }" @click="sortMode = 'nameDesc'; sortMenuOpen = false">{{ t("名称 Z–A") }}</button></div></div></div>
        <label class="search"><Search :size="17" /><input ref="searchInput" v-model="searchTerm" type="search" :placeholder="t('搜索名称、来源或标签')" /><kbd>Ctrl K</kbd></label>
        <div class="archive-list" :aria-busy="loading">
          <article v-for="item in filteredArchives" :key="item.id" class="archive-row" :class="{ selected: selectedArchiveId === item.id, 'needs-location': archiveNeedsLocation(item) }" draggable="true" tabindex="0" @dragstart="startArchiveDrag(item.id, $event)" @dragend="finishDrag" @click="selectArchive(item.id)" @keydown.enter="selectArchive(item.id)">
            <span class="file-icon"><Folder v-if="item.kind === 'folder'" :size="19" /><File v-else-if="item.kind === 'file'" :size="19" /><FolderArchive v-else :size="19" /></span>
            <span class="archive-copy"><span class="row-title"><button class="archive-name" :aria-label="t('编辑存档 {name}', { name: item.name })" :title="t('编辑存档 {name}', { name: item.name })" @click.stop="openArchiveEditor(item)">{{ item.name }}</button><span class="automation-badges"><button class="automation-badge" :class="{ active: item.autoBackupEnabled }" :aria-label="t('编辑 {name} 的自动备份设置', { name: item.name })" :title="t('编辑 {name} 的自动备份设置', { name: item.name })" @click.stop="openArchiveEditor(item)">{{ t("自动备份") }}</button><button class="automation-badge" :class="{ active: item.automaticUploadEnabled }" :aria-label="t('编辑 {name} 的自动上传设置', { name: item.name })" :title="t('编辑 {name} 的自动上传设置', { name: item.name })" @click.stop="openArchiveEditor(item)">{{ t("自动上传") }}</button></span><button v-if="archiveNeedsLocation(item)" class="location-warning" :aria-label="t('重新定位 {name} 的本机来源', { name: item.name })" :title="t('重新定位本机来源')" @click.stop="openArchiveEditor(item, true)"><AlertTriangle :size="13" /></button><i v-else :class="item.lastSnapshotAt ? 'synced' : 'local'"><Check v-if="item.lastSnapshotAt" :size="13" /><HardDrive v-else :size="13" /></i></span><small>{{ archiveNeedsLocation(item) ? t("等待定位本机来源") : displaySourcePath(item.sourcePath) }}</small><span class="meta"><span>{{ item.category }}</span><span>{{ formatBytes(item.totalBytes) }}</span><span>{{ formatTime(item.lastSnapshotAt) }}</span></span></span>
          </article>
          <div v-if="!loading && !archives.length" class="empty-state"><span class="empty-icon"><FolderArchive :size="26" /></span><b>{{ t("添加第一个存档") }}</b><p>{{ t("把一个或多个文件、文件夹组合为可查询和恢复的时间线。") }}</p><button @click="openCreateArchive"><Plus :size="16" />{{ t("添加存档") }}</button></div>
          <div v-else-if="!loading && !filteredArchives.length" class="empty"><Search :size="22" /><span>{{ t("没有找到匹配的存档") }}</span></div>
        </div>
        <button class="archive-panel-toggle" type="button" :aria-label="t('收起存档列表')" :title="t('收起存档列表')" :aria-expanded="true" @click="archivePanelCollapsed = true"><ChevronLeft :size="16" /></button>
      </section>
      <button v-if="archivePanelCollapsed" class="archive-panel-toggle collapsed" type="button" :aria-label="t('展开存档列表')" :title="t('展开存档列表')" :aria-expanded="false" @click="archivePanelCollapsed = false"><ChevronRight :size="16" /></button>

      <section v-if="selectedArchive" class="detail-panel" aria-labelledby="detail-title">
        <header class="detail-header">
          <div class="identity"><span class="detail-icon"><Folder v-if="selectedArchive.kind === 'folder'" /><File v-else-if="selectedArchive.kind === 'file'" /><FolderArchive v-else /></span><div class="title-line"><h2 id="detail-title"><button class="detail-archive-name" :title="t('编辑存档 {name}', { name: selectedArchive.name })" @click="openEditArchive">{{ selectedArchive.name }}</button></h2><span class="automation-badges"><button class="automation-badge" :class="{ active: selectedArchive.autoBackupEnabled }" :title="t('编辑 {name} 的自动备份设置', { name: selectedArchive.name })" @click="openEditArchive">{{ t("自动备份") }}</button><button class="automation-badge" :class="{ active: selectedArchive.automaticUploadEnabled }" :title="t('编辑 {name} 的自动上传设置', { name: selectedArchive.name })" @click="openEditArchive">{{ t("自动上传") }}</button></span></div></div>
          <div class="actions"><button data-tour="sync" class="secondary sync-progress-button" :style="{ '--sync-progress': currentSyncProgressFraction }" :disabled="syncingArchive || syncingAllArchives" @click="syncSelectedArchive"><span><UploadCloud :size="17" />{{ syncingArchive ? t("同步中 {syncProgressText}", { syncProgressText: syncProgressText }) : t("同步") }}</span></button><div class="more-control"><button class="icon-button" :aria-label="t('更多操作')" :title="t('更多操作')" :aria-expanded="archiveMenuOpen" @click="archiveMenuOpen = !archiveMenuOpen"><MoreHorizontal :size="19" /></button><div v-if="archiveMenuOpen" class="archive-actions-menu"><button @click="openSelectedArchiveSources"><FolderOpen :size="15" />{{ t("打开来源") }}</button><button @click="openSelectedArchiveStorage"><HardDrive :size="15" />{{ t("打开资料库") }}</button><button @click="openEditArchive"><Pencil :size="15" />{{ t("编辑存档") }}</button><button class="danger" @click="selectedArchive && deleteArchive(selectedArchive)"><Trash2 :size="15" />{{ t("删除存档") }}</button></div></div></div>
        </header>

        <ArchiveMetadata :archive="selectedArchive" :saving-tags="savingTags" @add-tag="addArchiveTag" @remove-tag="removeArchiveTag" />

        <div class="detail-content">
          <section class="timeline-area">
            <div class="section-title"><div><p class="label">{{ t("版本历史") }}</p><h3>{{ t("时间节点") }}</h3></div></div>
            <div class="snapshot-create"><input v-model="snapshotDescription" maxlength="160" :placeholder="t('输入新存档描述信息（可留空）')" @keydown.enter.prevent="createSnapshotFromDetail" /><button data-tour="snapshot" class="accent" :disabled="busyAction !== undefined" @click="createSnapshotFromDetail"><Plus :size="16" />{{ busyAction === 'snapshot' ? t("创建中") : t("创建新快照") }}</button></div>
            <div class="timeline-toolbar"><label><Search :size="15" /><input v-model="snapshotSearch" type="search" :placeholder="t('搜索快照描述')" /></label><ThemedSelect :model-value="snapshotSort" :options="timelineSortOptions" :label="t('时间线排序')" @update:model-value="updateTimelineSort" /></div>
            <div v-if="visibleSnapshots.length" data-tour="timeline" class="timeline-table"><div class="timeline-table-head"><span>{{ t("备份时间") }}</span><span>{{ t("描述") }}</span><span>{{ t("位置 / 大小") }}</span><span>{{ t("操作") }}</span></div><article v-for="snapshot in visibleSnapshots" :key="snapshot.id" :class="{ selected: selectedSnapshotId === snapshot.id }" tabindex="0" @click="selectedSnapshotId = snapshot.id" @keydown.enter="selectedSnapshotId = snapshot.id"><time>{{ formatTime(snapshot.createdAt) }}</time><span><b>{{ snapshot.title }}</b><small>{{ snapshot.note || snapshotDetail(snapshot) }}</small></span><span>{{ t("本机 ·") }} {{ formatBytes(snapshot.totalBytes) }}</span><div class="timeline-actions"><button data-tour="lock" class="snapshot-star" :class="{ locked: snapshot.locked }" :disabled="Boolean(lockingSnapshotId) || busyAction !== undefined" :aria-label="snapshot.locked ? t('解锁重要快照') : t('标记为重要快照')" :title="snapshot.locked ? t('重要快照：不会自动清理，点击解锁') : t('标记为重要快照，防止自动清理')" :aria-pressed="Boolean(snapshot.locked)" @click.stop="toggleSnapshotLock(snapshot)"><Star :size="16" :fill="snapshot.locked ? 'currentColor' : 'none'" /></button><button class="info" :class="{ active: activityPanelOpen && selectedSnapshotId === snapshot.id }" :aria-label="t('查看 {time} 的时间节点详情', { time: formatTime(snapshot.createdAt) })" :title="t('查看 {time} 的时间节点详情', { time: formatTime(snapshot.createdAt) })" :aria-pressed="activityPanelOpen && selectedSnapshotId === snapshot.id" @click.stop="selectedSnapshotId = snapshot.id; activityPanelOpen = true"><Info :size="16" /></button><button data-tour="restore" :disabled="busyAction !== undefined" :aria-label="t('恢复 {time} 的时间节点', { time: formatTime(snapshot.createdAt) })" :title="t('恢复 {time} 的时间节点', { time: formatTime(snapshot.createdAt) })" @click.stop="selectedSnapshotId = snapshot.id; restoreSnapshot()"><RotateCcw :size="16" /><span>{{ t("恢复") }}</span></button><button :disabled="syncingArchive || syncingAllArchives" :aria-label="t('同步 {time} 的时间节点', { time: formatTime(snapshot.createdAt) })" :title="t('同步 {time} 的时间节点', { time: formatTime(snapshot.createdAt) })" @click.stop="selectedSnapshotId = snapshot.id; syncSelectedArchive()"><UploadCloud :size="16" /><span>{{ t("同步") }}</span></button><button class="danger" :disabled="busyAction !== undefined || snapshot.locked || Boolean(lockingSnapshotId)" :aria-label="t('删除 {time} 的时间节点', { time: formatTime(snapshot.createdAt) })" :title="snapshot.locked ? t('请先解锁重要快照再删除') : t('删除 {time} 的时间节点', { time: formatTime(snapshot.createdAt) })" @click.stop="deleteSnapshot(snapshot)"><Trash2 :size="16" /></button></div></article></div>
            <div v-else class="timeline-empty"><Clock3 :size="25" /><b>{{ t("还没有时间节点") }}</b><p>{{ t("创建首个备份后，可以从这里查看和恢复历史版本。") }}</p><button :disabled="busyAction !== undefined" @click="createSnapshot('初始版本')">{{ t("创建首个备份") }}</button></div>
          </section>

          <aside class="inspector activity-panel" :class="{ expanded: activityPanelOpen && Boolean(selectedSnapshot) }">
            <template v-if="selectedSnapshot && activityPanelOpen">
              <div class="section-title"><div><p class="label">{{ t("已选版本") }}</p><h3>{{ formatTime(selectedSnapshot.createdAt) }}</h3></div><span class="verified"><Check :size="13" />{{ t("完整") }}</span><button class="inspector-close" :aria-label="t('关闭时间节点详情')" :title="t('关闭时间节点详情')" @click="activityPanelOpen = false"><X :size="16" /></button></div>
              <dl><div><dt>{{ t("类型") }}</dt><dd>{{ selectedSnapshot.title }}</dd></div><div><dt>{{ t("快照大小") }}</dt><dd>{{ formatBytes(selectedSnapshot.totalBytes) }}</dd></div><div><dt>{{ t("存储位置") }}</dt><dd>{{ t("仅本地") }}</dd></div><div><dt>{{ t("内容校验") }}</dt><dd class="hash">{{ selectedSnapshot.contentHash.slice(0, 6) }}…{{ selectedSnapshot.contentHash.slice(-4) }}</dd></div></dl>
              <div class="changes"><p>{{ t("内容变化") }}</p><div><span><i class="green"></i>{{ t("新增") }}</span><b>{{ selectedSnapshot.changes.added }}</b></div><div><span><i class="amber"></i>{{ t("修改") }}</span><b>{{ selectedSnapshot.changes.modified }}</b></div><div><span><i class="red"></i>{{ t("删除") }}</span><b>{{ selectedSnapshot.changes.deleted }}</b></div></div>
              <label class="snapshot-note"><span>{{ t("备注") }}</span><textarea v-model="snapshotNote" maxlength="500" :placeholder="t('记录当前进度、目标或注意事项')" @keydown.ctrl.enter.prevent="saveSnapshotNote"></textarea><button :disabled="savingSnapshotNote" @click="saveSnapshotNote"><Save :size="16" />{{ savingSnapshotNote ? t("保存中") : t("保存备注") }}</button></label>
              <div class="inspector-divider" aria-hidden="true"></div>
              <button class="restore" :disabled="busyAction !== undefined" @click="restoreSnapshot"><RotateCcw :size="16" />{{ busyAction === 'restore' ? t("正在恢复") : t("恢复到这个时间节点") }}</button><p class="hint">{{ t("恢复前会先创建当前状态的安全快照。") }}</p>
            </template>
            <div v-else class="inspector-empty"><Clock3 :size="20" /><span>{{ t("选择时间节点后，点击信息按钮查看详情") }}</span></div>
          </aside>
        </div>
      </section>

      <section v-else class="detail-panel detail-placeholder"><span><FolderArchive :size="31" /></span><h2>{{ t("本地云端通用文件快照管理器") }}</h2><p>{{ t("从左侧添加文件或文件夹，开始保存和恢复历史状态。") }}</p></section>
    </main>

    <AppToast v-if="notice" :message="notice.message" :type="notice.type" @close="notice = undefined" />
    <SettingsDialog v-if="settingsOpen" @restart-tutorial="restartTutorial" :update-checking="updateChecking" @close="settingsOpen = false" @saved="handleSettingsChanged" @check-update="checkForApplicationUpdate(true, $event)" />
    <SteamScanDialog v-if="steamScanOpen" @close="steamScanOpen = false" @saved="handleSettingsChanged" />
    <button class="floating-theme-toggle" :class="{ 'is-dark': appSettings.colorMode === 'dark' }" :aria-label="appSettings.colorMode === 'dark' ? t('切换到日间模式') : t('切换到夜间模式')" :title="appSettings.colorMode === 'dark' ? t('切换到日间模式') : t('切换到夜间模式')" :aria-pressed="appSettings.colorMode === 'dark'" @click="toggleColorMode"><Sun v-if="appSettings.colorMode === 'dark'" :size="17" /><Moon v-else :size="17" /></button>
    <UpdateDialog v-if="availableUpdate && !tutorialActive" :update="availableUpdate" @close="availableUpdate = undefined" />
    <CloudCenterDialog v-if="cloudSettingsOpen" @close="cloudSettingsOpen = false" @saved="handleCloudSettingsChanged" @downloaded="handleCloudDownload" @settings-downloaded="handleCloudSettingsDownload" />
    <CloudHealthDialog v-if="cloudHealthDialogOpen" :items="cloudHealthCheckItems" :running="cloudHealthCheckRunning" @close="cloudHealthDialogOpen = false" />
    <CreateCategoryDialog :tutorial-progress="tutorialProgress" @tutorial-tip="rememberTutorialTip"
      v-if="categoryDialogOpen"
      :parent-name="categoryRecords.find((category) => category.id === selectedCategoryId)?.name"
      :submitting="creatingCategory"
      :error="createCategoryError"
      @close="categoryDialogOpen = false"
      @submit="createCategory"
    />
    <TutorialOverlay v-if="tutorialActive && !steamScanOpen && !confirmRequest && !closeRequestOpen"  :language-saving="tutorialLanguageSaving" @language-change="changeTutorialLanguage" :appearance-page="tutorial.step === 'appearance'" :appearance="normalizeAppearance(appSettings)" :appearance-saving="tutorialAppearanceSaving" @appearance-change="changeTutorialAppearance" :welcome="tutorial.step === 'welcome'" :route="tutorial.step === 'route'" :finish="tutorial.step === 'finish'" :step="tutorialViews[tutorial.step]" :has-archive="Boolean(selectedArchive)" @start="tutorialEvent({ type: 'start' })" @skip="tutorialEvent({ type: 'skip' })" @local="localTutorial" @cloud="tutorialEvent({ type: 'choose-cloud' })" @next="nextTutorial" @use-existing="useExistingTutorial" />
    <CreateArchiveDialog :tutorial-progress="tutorialProgress" @tutorial-tip="rememberTutorialTip"
      v-if="createDialogOpen"
      :sources="pendingSources"
      :default-initial-snapshot="appSettings.createInitialSnapshot"
      :picking="pickingSource"
      :submitting="creatingArchive"
      :error="createArchiveError"
      :edit-name="editingArchive?.name"
      :edit-exclude-patterns="editingArchive?.excludePatterns"
      :edit-storage-policy="editingArchive?.storagePolicy"
      :edit-auto-backup-enabled="editingArchive?.autoBackupEnabled"
      :edit-automatic-upload-enabled="editingArchive?.automaticUploadEnabled"
      :highlight-sources="highlightSources"
      @close="closeArchiveDialog"
      @pick="pickSources"
      @registry="addRegistrySource"
      @remove="pendingSources = pendingSources.filter((source) => source.id !== $event)"
      @submit="createArchive"
    />
    <RegistryRestoreDialog v-if="registryRestoreRequest" :paths="registryRestoreRequest.archive.sources.filter((source) => source.kind === 'registry').map((source) => source.path)" @cancel="registryRestoreRequest = undefined" @confirm="confirmRegistryRestore" />
    <ConfirmDialog v-if="confirmRequest" :title="confirmRequest.title" :message="confirmRequest.message" :confirm-label="confirmRequest.confirmLabel" :destructive="confirmRequest.destructive" @cancel="answerConfirmation(false)" @confirm="answerConfirmation(true)" />
    <ConfirmDialog v-if="closeRequestOpen" :title="t('关闭 Chronicle')" :message="t('最小化到托盘后，自动备份仍会在后台运行。')" :confirm-label="t('最小化到托盘')" @cancel="closeRequestOpen = false" @confirm="hideMainWindowToTray"><template #extra-actions><button class="exit-button" @click="invoke('exit_chronicle')">{{ t("退出 Chronicle") }}</button></template></ConfirmDialog>
  </div>
</template>
