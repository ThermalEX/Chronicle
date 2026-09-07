<script setup lang="ts">
import {
  ArchiveRestore, Check, ChevronDown, ChevronRight, Clock3, CloudCog, File, FileClock,
  Folder, FolderArchive, HardDrive, LockKeyhole, MoreHorizontal, Plus, RotateCcw,
  Search, Settings2, SlidersHorizontal, UploadCloud, X,
} from "@lucide/vue";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import CloudSettingsDialog from "./components/CloudSettingsDialog.vue";
import ArchiveMetadata from "./components/ArchiveMetadata.vue";
import CreateCategoryDialog from "./components/CreateCategoryDialog.vue";
import CreateArchiveDialog from "./components/CreateArchiveDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import type { ArchiveRecord, ArchiveSource, CategoryRecord, CreateArchiveInput, SnapshotProgress, SnapshotRecord, SourceKind } from "./domain";
import { archiveRepository, isTauriRuntime } from "./services/repository";
import { compareArchiveNames, type ArchiveSortMode } from "./services/archiveSorting";
import { appSettings, initializeSettings, shortcutMatches } from "./services/settings";

const categoryDefinitions = [
  { name: "游戏", icon: ArchiveRestore },
  { name: "创作", icon: FileClock },
  { name: "工作", icon: Folder },
  { name: "配置", icon: Settings2 },
];

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
const snapshotProgress = ref<SnapshotProgress>();
const busyAction = ref<"snapshot" | "restore">();
const savingTags = ref(false);
const notice = ref<{ type: "success" | "error" | "info"; message: string }>();
let noticeTimer: number | undefined;

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
        icon: categoryDefinitions.find((item) => item.name === category.name)?.icon ?? Folder,
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
const progressPercent = computed(() => {
  if (!snapshotProgress.value?.total) return 0;
  return Math.round(snapshotProgress.value.current / snapshotProgress.value.total * 100);
});

function showNotice(message: string, type: "success" | "error" | "info" = "success") {
  notice.value = { message, type };
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => { notice.value = undefined; }, 3200);
}

function readableError(error: unknown): string {
  return error instanceof Error ? error.message : "操作失败";
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

async function refreshSnapshots(archiveId?: string) {
  snapshots.value = archiveId ? await archiveRepository.listSnapshots(archiveId) : [];
  selectedSnapshotId.value = snapshots.value[0]?.id;
}

function openCreateArchive() {
  pendingSources.value = [];
  createArchiveError.value = undefined;
  createDialogOpen.value = true;
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
    const archive = await archiveRepository.createArchive(input);
    createDialogOpen.value = false;
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
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
    selectedSnapshotId.value = snapshot.id;
    showNotice(`时间节点已创建，保存 ${snapshot.files.length} 个文件`);
  } catch (error) {
    showNotice(readableError(error), "error");
  } finally {
    busyAction.value = undefined;
    snapshotProgress.value = undefined;
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

function selectCategory(categoryId: string) {
  selectedCategoryId.value = categoryId;
  activeTreeNodeId.value = `category:${categoryId}`;
  selectedArchiveId.value = filteredArchives.value[0]?.id;
}

function selectArchive(archiveId: string) {
  selectedArchiveId.value = archiveId;
  activeTreeNodeId.value = `archive:${archiveId}`;
}

function selectArchiveFromTree(archive: ArchiveRecord) {
  selectedCategoryId.value = archive.categoryId ?? "all";
  selectArchive(archive.id);
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
    createDialogOpen.value = false;
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

watch(selectedArchiveId, (archiveId) => { void refreshSnapshots(archiveId); });
onMounted(async () => {
  window.addEventListener("keydown", handleShortcut);
  try {
    await initializeSettings();
    await refreshCategories();
    await refreshArchives();
  }
  catch (error) { showNotice(readableError(error), "error"); }
  finally { loading.value = false; }
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleShortcut);
  window.clearTimeout(noticeTimer);
});
</script>

<template>
  <div class="app-shell">
    <header class="titlebar">
      <div class="brand"><span class="brand-mark"><Clock3 :size="18" /></span><span>Chronicle</span></div>
      <div class="sync-state"><i></i>本地资料库可用</div>
      <div class="toolbar"><button aria-label="云端设置" @click="cloudSettingsOpen = true"><CloudCog :size="18" /></button><button aria-label="应用设置" @click="settingsOpen = true"><Settings2 :size="18" /></button></div>
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
            <button v-if="node.id !== 'all'" class="disclosure" :class="{ hidden: !node.hasChildren }" :aria-label="`${expandedCategoryIds.has(node.id) ? '折叠' : '展开'} ${node.name}`" :aria-expanded="node.hasChildren ? expandedCategoryIds.has(node.id) : undefined" @click="toggleCategory(node.id)"><ChevronDown v-if="expandedCategoryIds.has(node.id)" :size="14" /><ChevronRight v-else :size="14" /></button>
            <span v-else class="disclosure-spacer"></span>
            <button class="category-select" :draggable="node.id !== 'all'" :title="node.id === 'all' ? '将分类或存档拖到这里可移至根目录' : undefined" @dragstart.stop="node.id !== 'all' && startCategoryDrag(node.id, $event)" @click="selectCategory(node.id)"><component :is="node.icon" :size="17" /><span>{{ node.name }}</span><span class="category-suffix"><LockKeyhole v-if="node.id === 'all'" :size="12" aria-label="固定根目录" /><small>{{ node.count }}</small></span></button>
          </div>
          <div v-else class="archive-tree-row" :class="{ active: activeTreeNodeId === `archive:${node.id}` }" :style="{ paddingLeft: `${4 + node.depth * 16}px` }" @dragend="finishDrag">
            <span class="disclosure-spacer"></span>
            <button class="archive-tree-select" draggable="true" :title="node.archive.name" @dragstart.stop="startArchiveDrag(node.id, $event)" @click="selectArchiveFromTree(node.archive)"><File :size="16" /><span>{{ node.archive.name }}</span></button>
          </div>
        </template>
      </nav>
      <div class="spacer"></div>
      <section class="storage"><div><HardDrive :size="17" /><span>本地快照</span><b>{{ formatBytes(snapshots.reduce((sum, item) => sum + item.totalBytes, 0)) }}</b></div><small>数据保存在 Chronicle 本地资料库</small></section>
      <button class="account"><span class="avatar">T</span><span><b>ThermalEX</b><small>本机设备</small></span><ChevronDown :size="16" /></button>
    </aside>

    <main class="workspace">
      <section class="archive-panel" aria-labelledby="archives-title">
        <div class="panel-title"><div><p class="label">{{ selectedCategoryName }}</p><h1 id="archives-title">存档</h1></div><div class="sort-control"><button class="icon-button" aria-label="排列方式" :aria-expanded="sortMenuOpen" @click="sortMenuOpen = !sortMenuOpen"><SlidersHorizontal :size="18" /></button><div v-if="sortMenuOpen" class="sort-menu"><button :class="{ active: sortMode === 'newest' }" @click="sortMode = 'newest'; sortMenuOpen = false">时间 新–旧</button><button :class="{ active: sortMode === 'oldest' }" @click="sortMode = 'oldest'; sortMenuOpen = false">时间 旧–新</button><button :class="{ active: sortMode === 'nameAsc' }" @click="sortMode = 'nameAsc'; sortMenuOpen = false">名称 A–Z</button><button :class="{ active: sortMode === 'nameDesc' }" @click="sortMode = 'nameDesc'; sortMenuOpen = false">名称 Z–A</button></div></div></div>
        <label class="search"><Search :size="17" /><input ref="searchInput" v-model="searchTerm" type="search" placeholder="搜索名称、来源或标签" /><kbd>Ctrl K</kbd></label>
        <div class="archive-list" :aria-busy="loading">
          <button v-for="item in filteredArchives" :key="item.id" class="archive-row" :class="{ selected: selectedArchiveId === item.id }" draggable="true" @dragstart="startArchiveDrag(item.id, $event)" @dragend="finishDrag" @click="selectArchive(item.id)">
            <span class="file-icon"><Folder v-if="item.kind === 'folder'" :size="19" /><File v-else-if="item.kind === 'file'" :size="19" /><FolderArchive v-else :size="19" /></span>
            <span class="archive-copy"><span class="row-title"><b>{{ item.name }}</b><i :class="item.lastSnapshotAt ? 'synced' : 'local'"><Check v-if="item.lastSnapshotAt" :size="13" /><HardDrive v-else :size="13" /></i></span><small>{{ displaySourcePath(item.sourcePath) }}</small><span class="meta"><span>{{ item.category }}</span><span>{{ formatBytes(item.totalBytes) }}</span><span>{{ formatTime(item.lastSnapshotAt) }}</span></span></span>
          </button>
          <div v-if="!loading && !archives.length" class="empty-state"><span class="empty-icon"><FolderArchive :size="26" /></span><b>添加第一个存档</b><p>把一个或多个文件、文件夹组合为可查询和恢复的时间线。</p><button @click="openCreateArchive"><Plus :size="16" />添加存档</button></div>
          <div v-else-if="!loading && !filteredArchives.length" class="empty"><Search :size="22" /><span>没有找到匹配的存档</span></div>
        </div>
      </section>

      <section v-if="selectedArchive" class="detail-panel" aria-labelledby="detail-title">
        <header class="detail-header">
          <div class="identity"><span class="detail-icon"><Folder v-if="selectedArchive.kind === 'folder'" /><File v-else-if="selectedArchive.kind === 'file'" /><FolderArchive v-else /></span><div class="title-line"><h2 id="detail-title">{{ selectedArchive.name }}</h2></div></div>
          <div class="actions"><button class="secondary" @click="cloudSettingsOpen = true"><UploadCloud :size="17" />同步</button><button class="accent" :disabled="busyAction !== undefined" @click="createSnapshot()"><Plus :size="17" />{{ busyAction === 'snapshot' ? '创建中' : '创建备份' }}</button><button class="icon-button" aria-label="更多操作"><MoreHorizontal :size="19" /></button></div>
        </header>

        <ArchiveMetadata :archive="selectedArchive" :saving-tags="savingTags" @add-tag="addArchiveTag" @remove-tag="removeArchiveTag" />

        <div v-if="snapshotProgress" class="operation-progress" aria-live="polite"><span><b>正在读取文件</b><small>{{ snapshotProgress.currentPath || '正在完成校验' }}</small></span><strong>{{ progressPercent }}%</strong><progress :value="snapshotProgress.current" :max="snapshotProgress.total || 1"></progress></div>

        <div class="detail-content">
          <section class="timeline-area">
            <div class="section-title"><div><p class="label">版本历史</p><h3>时间节点</h3></div><button class="link-button" @click="showNotice('保留策略将在自动备份阶段接入', 'info')">管理保留策略</button></div>
            <div v-if="snapshots.length" class="timeline">
              <button v-for="snapshot in snapshots" :key="snapshot.id" class="snapshot" :class="{ selected: selectedSnapshotId === snapshot.id }" @click="selectedSnapshotId = snapshot.id"><span class="rail"><i></i></span><span class="snapshot-copy"><span><b>{{ snapshot.title }}</b><time>{{ formatTime(snapshot.createdAt) }}</time></span><small>{{ snapshotDetail(snapshot) }}</small><em><HardDrive :size="13" />仅本地 · {{ snapshot.files.length }} 个文件</em></span><strong>{{ formatBytes(snapshot.totalBytes) }}</strong></button>
            </div>
            <div v-else class="timeline-empty"><Clock3 :size="25" /><b>还没有时间节点</b><p>创建首个备份后，可以从这里查看和恢复历史版本。</p><button :disabled="busyAction !== undefined" @click="createSnapshot('初始版本')">创建首个备份</button></div>
          </section>

          <aside class="inspector">
            <template v-if="selectedSnapshot">
              <div class="section-title"><div><p class="label">已选版本</p><h3>{{ formatTime(selectedSnapshot.createdAt) }}</h3></div><span class="verified"><Check :size="13" />完整</span></div>
              <dl><div><dt>类型</dt><dd>{{ selectedSnapshot.title }}</dd></div><div><dt>快照大小</dt><dd>{{ formatBytes(selectedSnapshot.totalBytes) }}</dd></div><div><dt>存储位置</dt><dd>仅本地</dd></div><div><dt>内容校验</dt><dd class="hash">{{ selectedSnapshot.contentHash.slice(0, 6) }}…{{ selectedSnapshot.contentHash.slice(-4) }}</dd></div></dl>
              <div class="changes"><p>内容变化</p><div><span><i class="green"></i>新增</span><b>{{ selectedSnapshot.changes.added }}</b></div><div><span><i class="amber"></i>修改</span><b>{{ selectedSnapshot.changes.modified }}</b></div><div><span><i class="red"></i>删除</span><b>{{ selectedSnapshot.changes.deleted }}</b></div></div>
              <button class="restore" :disabled="busyAction !== undefined" @click="restoreSnapshot"><RotateCcw :size="17" />{{ busyAction === 'restore' ? '正在恢复' : '恢复到这个时间节点' }}</button><p class="hint">恢复前会先创建当前状态的安全快照。</p>
            </template>
            <div v-else class="inspector-empty"><Clock3 :size="20" /><span>选择时间节点后显示详情</span></div>
          </aside>
        </div>
      </section>

      <section v-else class="detail-panel detail-placeholder"><span><FolderArchive :size="31" /></span><h2>本地优先的时间节点管理</h2><p>从左侧添加文件或文件夹，开始保存和恢复历史状态。</p></section>
    </main>

    <div v-if="notice" class="toast" :class="notice.type" role="status"><span>{{ notice.message }}</span><button aria-label="关闭通知" @click="notice = undefined"><X :size="15" /></button></div>
    <SettingsDialog v-if="settingsOpen" @close="settingsOpen = false" @saved="showNotice('设置已保存')" />
    <CloudSettingsDialog v-if="cloudSettingsOpen" @close="cloudSettingsOpen = false" @saved="showNotice('云端设置已保存')" />
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
      @close="createDialogOpen = false"
      @pick="pickSources"
      @remove="pendingSources = pendingSources.filter((source) => source.id !== $event)"
      @submit="createArchive"
    />
  </div>
</template>
