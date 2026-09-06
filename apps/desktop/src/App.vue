<script setup lang="ts">
import {
  ArchiveRestore, Check, ChevronDown, Clock3, CloudCog, File, FileClock,
  Folder, FolderArchive, HardDrive, MoreHorizontal, Plus, RotateCcw,
  Search, Settings2, SlidersHorizontal, Tag, UploadCloud, X,
} from "@lucide/vue";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { ArchiveKind, ArchiveRecord, SnapshotProgress, SnapshotRecord } from "./domain";
import { archiveRepository } from "./services/archiveRepository";

const categoryDefinitions = [
  { name: "游戏", icon: ArchiveRestore },
  { name: "创作", icon: FileClock },
  { name: "工作", icon: Folder },
  { name: "配置", icon: Settings2 },
  { name: "未分类", icon: FolderArchive },
];

const archives = ref<ArchiveRecord[]>([]);
const snapshots = ref<SnapshotRecord[]>([]);
const selectedCategory = ref("全部存档");
const selectedArchiveId = ref<string>();
const selectedSnapshotId = ref<string>();
const searchTerm = ref("");
const searchInput = ref<HTMLInputElement>();
const loading = ref(true);
const addMenuOpen = ref(false);
const snapshotProgress = ref<SnapshotProgress>();
const busyAction = ref<"snapshot" | "restore">();
const notice = ref<{ type: "success" | "error" | "info"; message: string }>();
let noticeTimer: number | undefined;

const categories = computed(() => [
  { name: "全部存档", count: archives.value.length, icon: FolderArchive },
  ...categoryDefinitions.map((category) => ({
    ...category,
    count: archives.value.filter((archive) => archive.category === category.name).length,
  })).filter((category) => category.count > 0),
]);
const filteredArchives = computed(() => {
  const query = searchTerm.value.trim().toLocaleLowerCase();
  return archives.value.filter((archive) =>
    (selectedCategory.value === "全部存档" || archive.category === selectedCategory.value) &&
    (!query || `${archive.name} ${archive.sourcePath} ${archive.category}`.toLocaleLowerCase().includes(query)),
  );
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

async function refreshSnapshots(archiveId?: string) {
  snapshots.value = archiveId ? await archiveRepository.listSnapshots(archiveId) : [];
  selectedSnapshotId.value = snapshots.value[0]?.id;
}

async function addArchive(kind: ArchiveKind) {
  addMenuOpen.value = false;
  if (!("showOpenFilePicker" in window) || !("showDirectoryPicker" in window)) {
    showNotice("当前环境不支持本地文件系统访问，请使用 Edge 或桌面版 Chronicle", "error");
    return;
  }
  try {
    const archive = await archiveRepository.addArchive(kind);
    if (!archive) return;
    await refreshArchives(archive.id);
    await refreshSnapshots(archive.id);
    showNotice(`已添加${kind === "file" ? "文件" : "文件夹"}“${archive.name}”`);
  } catch (error) {
    showNotice(readableError(error), "error");
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

function selectCategory(name: string) {
  selectedCategory.value = name;
  selectedArchiveId.value = filteredArchives.value[0]?.id;
}

function handleShortcut(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLocaleLowerCase() === "k") {
    event.preventDefault();
    void nextTick(() => searchInput.value?.focus());
  }
  if (event.key === "Escape") addMenuOpen.value = false;
}

watch(selectedArchiveId, (archiveId) => { void refreshSnapshots(archiveId); });
onMounted(async () => {
  window.addEventListener("keydown", handleShortcut);
  try { await refreshArchives(); }
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
      <div class="toolbar"><button aria-label="同步设置" @click="showNotice('WebDAV 将在下一个里程碑接入', 'info')"><CloudCog :size="18" /></button><button aria-label="应用设置" @click="showNotice('设置模块正在建设中', 'info')"><Settings2 :size="18" /></button></div>
    </header>

    <aside class="sidebar">
      <div class="add-control">
        <button class="add-button" :aria-expanded="addMenuOpen" @click="addMenuOpen = !addMenuOpen"><Plus :size="18" />添加存档</button>
        <div v-if="addMenuOpen" class="add-menu">
          <button @click="addArchive('file')"><File :size="17" /><span><b>选择文件</b><small>管理单个配置或文档</small></span></button>
          <button @click="addArchive('folder')"><Folder :size="17" /><span><b>选择文件夹</b><small>管理完整目录树</small></span></button>
        </div>
      </div>
      <nav aria-label="存档分类">
        <p class="label">资料库</p>
        <button v-for="category in categories" :key="category.name" :class="{ active: selectedCategory === category.name }" @click="selectCategory(category.name)"><component :is="category.icon" :size="18" /><span>{{ category.name }}</span><small>{{ category.count }}</small></button>
      </nav>
      <div class="spacer"></div>
      <section class="storage"><div><HardDrive :size="17" /><span>本地快照</span><b>{{ formatBytes(snapshots.reduce((sum, item) => sum + item.totalBytes, 0)) }}</b></div><small>数据保存在浏览器的 Chronicle 本地资料库</small></section>
      <button class="account"><span class="avatar">T</span><span><b>ThermalEX</b><small>本机设备</small></span><ChevronDown :size="16" /></button>
    </aside>

    <main class="workspace">
      <section class="archive-panel" aria-labelledby="archives-title">
        <div class="panel-title"><div><p class="label">{{ selectedCategory }}</p><h1 id="archives-title">存档</h1></div><button class="icon-button" aria-label="筛选"><SlidersHorizontal :size="18" /></button></div>
        <label class="search"><Search :size="17" /><input ref="searchInput" v-model="searchTerm" type="search" placeholder="搜索名称或分类" /><kbd>Ctrl K</kbd></label>
        <div class="archive-list" :aria-busy="loading">
          <button v-for="item in filteredArchives" :key="item.id" class="archive-row" :class="{ selected: selectedArchiveId === item.id }" @click="selectedArchiveId = item.id">
            <span class="file-icon"><Folder v-if="item.kind === 'folder'" :size="19" /><File v-else :size="19" /></span>
            <span class="archive-copy"><span class="row-title"><b>{{ item.name }}</b><i :class="item.lastSnapshotAt ? 'synced' : 'local'"><Check v-if="item.lastSnapshotAt" :size="13" /><HardDrive v-else :size="13" /></i></span><small>{{ item.sourcePath }}</small><span class="meta"><span>{{ item.category }}</span><span>{{ formatBytes(item.totalBytes) }}</span><span>{{ formatTime(item.lastSnapshotAt) }}</span></span></span>
          </button>
          <div v-if="!loading && !archives.length" class="empty-state"><span class="empty-icon"><FolderArchive :size="26" /></span><b>添加第一个存档</b><p>选择一个文件或文件夹，Chronicle 会保存它的多个时间节点。</p><button @click="addMenuOpen = true"><Plus :size="16" />添加存档</button></div>
          <div v-else-if="!loading && !filteredArchives.length" class="empty"><Search :size="22" /><span>没有找到匹配的存档</span></div>
        </div>
      </section>

      <section v-if="selectedArchive" class="detail-panel" aria-labelledby="detail-title">
        <header class="detail-header">
          <div class="identity"><span class="detail-icon"><Folder v-if="selectedArchive.kind === 'folder'" /><File v-else /></span><div><div class="title-line"><h2 id="detail-title">{{ selectedArchive.name }}</h2><span class="tag"><Tag :size="12" />{{ selectedArchive.category }}</span></div><p>{{ selectedArchive.sourcePath }}</p></div></div>
          <div class="actions"><button class="secondary" @click="showNotice('WebDAV 将在下一个里程碑接入', 'info')"><UploadCloud :size="17" />同步</button><button class="accent" :disabled="busyAction !== undefined" @click="createSnapshot()"><Plus :size="17" />{{ busyAction === 'snapshot' ? '创建中' : '创建备份' }}</button><button class="icon-button" aria-label="更多操作"><MoreHorizontal :size="19" /></button></div>
        </header>

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
  </div>
</template>
