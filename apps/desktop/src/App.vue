<script setup lang="ts">
import {
  ArchiveRestore, Check, ChevronDown, Clock3, Cloud, CloudCog, File,
  FileClock, Folder, FolderArchive, HardDrive, MoreHorizontal, Plus,
  RotateCcw, Search, Settings2, SlidersHorizontal, Tag, UploadCloud,
} from "@lucide/vue";
import { computed, ref } from "vue";

type Archive = {
  id: number; name: string; path: string; category: string;
  kind: "folder" | "file"; size: string; updated: string;
  sync: "synced" | "local" | "syncing"; color: string;
};

const categories = [
  { name: "全部存档", count: 8, icon: FolderArchive },
  { name: "游戏", count: 3, icon: ArchiveRestore },
  { name: "创作", count: 2, icon: FileClock },
  { name: "工作", count: 2, icon: Folder },
  { name: "配置", count: 1, icon: Settings2 },
];

const archives: Archive[] = [
  { id: 1, name: "Affinity Designer", path: "D:\\Design\\Brand Project", category: "创作", kind: "folder", size: "284 MB", updated: "刚刚", sync: "synced", color: "#0f766e" },
  { id: 2, name: "Stardew Valley", path: "%APPDATA%\\StardewValley\\Saves", category: "游戏", kind: "folder", size: "42 MB", updated: "18 分钟前", sync: "synced", color: "#b45309" },
  { id: 3, name: "VS Code 设置", path: "%APPDATA%\\Code\\User\\settings.json", category: "配置", kind: "file", size: "18 KB", updated: "昨天", sync: "local", color: "#2563eb" },
  { id: 4, name: "论文资料", path: "D:\\Documents\\Research", category: "工作", kind: "folder", size: "1.8 GB", updated: "9 月 5 日", sync: "syncing", color: "#7c3aed" },
];

const snapshots = [
  { id: 1, title: "自动备份", detail: "检测到 6 个文件发生变化", time: "今天 02:14", size: "+12.6 MB", location: "本地与 WebDAV" },
  { id: 2, title: "调整品牌主视觉", detail: "手动创建 · 备注已保存", time: "昨天 21:36", size: "+48.2 MB", location: "本地与 WebDAV" },
  { id: 3, title: "自动备份", detail: "检测到 2 个文件发生变化", time: "9 月 5 日 18:20", size: "+3.4 MB", location: "本地与 WebDAV" },
  { id: 4, title: "项目初始版本", detail: "首次完整备份", time: "9 月 2 日 10:08", size: "219.8 MB", location: "仅本地" },
];

const selectedCategory = ref("全部存档");
const selectedArchiveId = ref(1);
const selectedSnapshotId = ref(1);
const searchTerm = ref("");
const syncRunning = ref(false);

const filteredArchives = computed(() => {
  const query = searchTerm.value.trim().toLowerCase();
  return archives.filter((item) =>
    (selectedCategory.value === "全部存档" || item.category === selectedCategory.value) &&
    (!query || `${item.name} ${item.path} ${item.category}`.toLowerCase().includes(query)),
  );
});
const selectedArchive = computed(() => archives.find((item) => item.id === selectedArchiveId.value) ?? archives[0]);
const selectedSnapshot = computed(() => snapshots.find((item) => item.id === selectedSnapshotId.value) ?? snapshots[0]);

function selectCategory(name: string) {
  selectedCategory.value = name;
  const first = archives.find((item) => name === "全部存档" || item.category === name);
  if (first) selectedArchiveId.value = first.id;
}

function runSync() {
  if (syncRunning.value) return;
  syncRunning.value = true;
  window.setTimeout(() => { syncRunning.value = false; }, 1400);
}
</script>

<template>
  <div class="app-shell">
    <header class="titlebar">
      <div class="brand"><span class="brand-mark"><Clock3 :size="18" /></span><span>Chronicle</span></div>
      <div class="sync-state" aria-live="polite"><i :class="{ pulse: syncRunning }"></i>{{ syncRunning ? "正在同步 WebDAV" : "所有更改已同步" }}</div>
      <div class="toolbar"><button aria-label="同步设置"><CloudCog :size="18" /></button><button aria-label="应用设置"><Settings2 :size="18" /></button></div>
    </header>

    <aside class="sidebar">
      <button class="add-button"><Plus :size="18" />添加存档</button>
      <nav aria-label="存档分类">
        <p class="label">资料库</p>
        <button v-for="category in categories" :key="category.name" :class="{ active: selectedCategory === category.name }" @click="selectCategory(category.name)">
          <component :is="category.icon" :size="18" /><span>{{ category.name }}</span><small>{{ category.count }}</small>
        </button>
      </nav>
      <div class="spacer"></div>
      <section class="storage">
        <div><HardDrive :size="17" /><span>本地存储</span><b>34%</b></div>
        <progress value="34" max="100">34%</progress><small>27.2 GB / 80 GB</small>
      </section>
      <button class="account"><span class="avatar">T</span><span><b>ThermalEX</b><small>本机设备</small></span><ChevronDown :size="16" /></button>
    </aside>

    <main class="workspace">
      <section class="archive-panel" aria-labelledby="archives-title">
        <div class="panel-title"><div><p class="label">{{ selectedCategory }}</p><h1 id="archives-title">存档</h1></div><button class="icon-button" aria-label="筛选"><SlidersHorizontal :size="18" /></button></div>
        <label class="search"><Search :size="17" /><input v-model="searchTerm" type="search" placeholder="搜索名称、路径或分类" /><kbd>Ctrl K</kbd></label>
        <div class="archive-list">
          <button v-for="item in filteredArchives" :key="item.id" class="archive-row" :class="{ selected: selectedArchiveId === item.id }" @click="selectedArchiveId = item.id">
            <span class="file-icon" :style="{ color: item.color, background: `${item.color}12` }"><Folder v-if="item.kind === 'folder'" :size="19" /><File v-else :size="19" /></span>
            <span class="archive-copy"><span class="row-title"><b>{{ item.name }}</b><i :class="item.sync"><Check v-if="item.sync === 'synced'" :size="13" /><UploadCloud v-else-if="item.sync === 'syncing'" :size="13" /><HardDrive v-else :size="13" /></i></span><small>{{ item.path }}</small><span class="meta"><span>{{ item.category }}</span><span>{{ item.size }}</span><span>{{ item.updated }}</span></span></span>
          </button>
          <div v-if="!filteredArchives.length" class="empty"><Search :size="22" /><span>没有找到匹配的存档</span></div>
        </div>
      </section>

      <section class="detail-panel" aria-labelledby="detail-title">
        <header class="detail-header">
          <div class="identity"><span class="detail-icon" :style="{ color: selectedArchive.color, background: `${selectedArchive.color}12` }"><Folder v-if="selectedArchive.kind === 'folder'" /><File v-else /></span><div><div class="title-line"><h2 id="detail-title">{{ selectedArchive.name }}</h2><span class="tag"><Tag :size="12" />{{ selectedArchive.category }}</span></div><p>{{ selectedArchive.path }}</p></div></div>
          <div class="actions"><button class="secondary" @click="runSync"><Cloud :size="17" />{{ syncRunning ? "同步中" : "立即同步" }}</button><button class="accent"><Plus :size="17" />创建备份</button><button class="icon-button" aria-label="更多操作"><MoreHorizontal :size="19" /></button></div>
        </header>

        <div class="detail-content">
          <section class="timeline-area">
            <div class="section-title"><div><p class="label">版本历史</p><h3>时间节点</h3></div><button class="link-button">管理保留策略</button></div>
            <div class="timeline">
              <button v-for="snapshot in snapshots" :key="snapshot.id" class="snapshot" :class="{ selected: selectedSnapshotId === snapshot.id }" @click="selectedSnapshotId = snapshot.id">
                <span class="rail"><i></i></span><span class="snapshot-copy"><span><b>{{ snapshot.title }}</b><time>{{ snapshot.time }}</time></span><small>{{ snapshot.detail }}</small><em><Cloud v-if="snapshot.location.includes('WebDAV')" :size="13" /><HardDrive v-else :size="13" />{{ snapshot.location }}</em></span><strong>{{ snapshot.size }}</strong>
              </button>
            </div>
          </section>

          <aside class="inspector">
            <div class="section-title"><div><p class="label">已选版本</p><h3>{{ selectedSnapshot.time }}</h3></div><span class="verified"><Check :size="13" />完整</span></div>
            <dl><div><dt>类型</dt><dd>{{ selectedSnapshot.title }}</dd></div><div><dt>大小变化</dt><dd>{{ selectedSnapshot.size }}</dd></div><div><dt>存储位置</dt><dd>{{ selectedSnapshot.location }}</dd></div><div><dt>内容校验</dt><dd class="hash">8f2a…c19d</dd></div></dl>
            <div class="changes"><p>内容变化</p><div><span><i class="green"></i>新增</span><b>4</b></div><div><span><i class="amber"></i>修改</span><b>2</b></div><div><span><i class="red"></i>删除</span><b>0</b></div></div>
            <button class="restore"><RotateCcw :size="17" />恢复到这个时间节点</button><p class="hint">恢复前会先创建当前状态的安全快照。</p>
          </aside>
        </div>
      </section>
    </main>
  </div>
</template>
