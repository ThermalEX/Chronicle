<script setup lang="ts">
import { BellRing, ClipboardCopy, FolderOpen, HardDrive, Info, Keyboard, RotateCcw, Settings2, Trash2, Undo2, X } from "@lucide/vue";
import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { computed, onMounted, reactive, ref, watch } from "vue";
import { appSettings, resetAppSettings, saveAppSettings, type AppSettings, type BackupSchedule, type CloseBehavior } from "../services/settings";
import { type ColorMode, type ColorTheme } from "../services/appearance";
import { archiveRepository } from "../services/repository";
import { createBackdropDismissal } from "../services/dialogDismissal";
import { diagnosticsRepository, type DiagnosticEntry } from "../services/diagnostics";
import type { RecycleItem } from "../domain";
import ConfirmDialog from "./ConfirmDialog.vue";
import ShortcutRecorder from "./ShortcutRecorder.vue";
import ThemedSelect, { type ThemedSelectOption } from "./ThemedSelect.vue";
import appIcon from "../assets/icon.png";
import { appMetadata } from "../services/appMetadata";

const emit = defineEmits<{ close: []; saved: [] }>();
const activeSection = ref<"software" | "notifications" | "backup" | "recycle" | "hotkeys" | "about">("software");
const closeButton = ref<HTMLButtonElement>();
const draft = reactive<AppSettings>({ ...appSettings });
const saving = ref(false);
const backdrop = createBackdropDismissal(() => emit("close"), () => !saving.value);
const recycleItems = ref<RecycleItem[]>([]);
const recycleBusy = ref(false);
const recycleError = ref("");
const recycleLocationError = ref("");
const diagnostics = ref<DiagnosticEntry[]>([]);
const diagnosticsBusy = ref(false);
const diagnosticsError = ref("");
const repositoryPath = ref("");
const confirmAction = ref<{ title: string; message: string; run: () => Promise<void> }>();
const recycleLocation = computed(() => draft.recycleBinPath || (repositoryPath.value ? `${repositoryPath.value}\\recycle` : "Chronicle\\recycle"));
const closeBehaviorOptions: ThemedSelectOption[] = [
  { value: "ask", label: "每次询问" },
  { value: "tray", label: "最小化到托盘" },
  { value: "exit", label: "退出 Chronicle" },
];
const backupScheduleOptions: ThemedSelectOption[] = [
  { value: "off", label: "关闭" },
  { value: "15m", label: "每 15 分钟" },
  { value: "1h", label: "每小时" },
  { value: "6h", label: "每 6 小时" },
  { value: "daily", label: "每天" },
];
const colorThemeOptions: ThemedSelectOption[] = [
  { value: "teal", label: "青绿" },
  { value: "indigo", label: "靛蓝" },
  { value: "violet", label: "紫罗兰" },
  { value: "amber", label: "琥珀" },
  { value: "rose", label: "玫红" },
  { value: "gray", label: "灰色" },
];
const colorModeOptions: ThemedSelectOption[] = [
  { value: "light", label: "日间模式" },
  { value: "dark", label: "夜间模式" },
];

const sections = [
  { id: "software" as const, label: "软件", icon: Settings2 },
  { id: "notifications" as const, label: "通知与错误", icon: BellRing },
  { id: "backup" as const, label: "存储与备份", icon: HardDrive },
  { id: "recycle" as const, label: "回收站", icon: Trash2 },
  { id: "hotkeys" as const, label: "热键", icon: Keyboard },
  { id: "about" as const, label: "关于", icon: Info },
];

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

async function loadDiagnostics(): Promise<void> {
  diagnosticsBusy.value = true;
  diagnosticsError.value = "";
  try { diagnostics.value = await diagnosticsRepository.list(); }
  catch (error) { diagnosticsError.value = error instanceof Error ? error.message : String(error); }
  finally { diagnosticsBusy.value = false; }
}

async function copyDiagnostics(entry?: DiagnosticEntry): Promise<void> {
  const text = entry
    ? `${entry.occurredAt}\n${entry.operation}\n${entry.message}\n${entry.details}`
    : diagnostics.value.map((item) => `${item.occurredAt}\n${item.operation}\n${item.message}\n${item.details}`).join("\n\n");
  try { await navigator.clipboard.writeText(text); }
  catch (error) { diagnosticsError.value = error instanceof Error ? error.message : "无法复制错误记录"; }
}

async function clearDiagnostics(): Promise<void> {
  if (!diagnostics.value.length || !window.confirm("清空全部错误记录？此操作不会影响存档和同步设置。")) return;
  diagnosticsBusy.value = true;
  try { await diagnosticsRepository.clear(); diagnostics.value = []; }
  catch (error) { diagnosticsError.value = error instanceof Error ? error.message : String(error); }
  finally { diagnosticsBusy.value = false; }
}

async function loadRecycleItems(): Promise<void> {
  recycleBusy.value = true;
  recycleError.value = "";
  try { recycleItems.value = await archiveRepository.listRecycleItems(); }
  catch (error) { recycleError.value = error instanceof Error ? error.message : String(error); }
  finally { recycleBusy.value = false; }
}

async function runRecycleAction(): Promise<void> {
  const action = confirmAction.value;
  confirmAction.value = undefined;
  if (!action) return;
  recycleBusy.value = true;
  try { await action.run(); await loadRecycleItems(); emit("saved"); }
  catch (error) { recycleError.value = error instanceof Error ? error.message : String(error); }
  finally { recycleBusy.value = false; }
}

function restoreItem(item: RecycleItem): void {
  confirmAction.value = { title: "恢复项目", message: `将“${item.displayName}”恢复到资料库。`, run: () => archiveRepository.restoreRecycleItem(item.id) };
}

function deleteItem(item: RecycleItem): void {
  confirmAction.value = { title: "永久删除", message: `“${item.displayName}”及其全部备份将永久删除，无法恢复。`, run: () => archiveRepository.permanentlyDeleteRecycleItem(item.id) };
}

function emptyRecycleBin(): void {
  confirmAction.value = { title: "清空回收站", message: "回收站中的全部存档和分类将永久删除，无法恢复。", run: () => archiveRepository.emptyRecycleBin() };
}

async function save(): Promise<void> {
  if (draft.retentionCount !== null) {
    draft.retentionCount = Math.max(1, Math.min(999, Number(draft.retentionCount) || 30));
  }
  saving.value = true;
  try {
    await saveAppSettings({ ...draft });
    emit("saved");
    emit("close");
  } finally {
    saving.value = false;
  }
}

function toggleRetentionLimit(event: Event): void {
  const unlimited = (event.target as HTMLInputElement).checked;
  draft.retentionCount = unlimited ? null : 30;
}

function updateCloseBehavior(value: string | null): void {
  if (value) draft.closeBehavior = value as CloseBehavior;
}

function updateBackupSchedule(value: string | null): void {
  if (value) draft.backupSchedule = value as BackupSchedule;
}

function updateColorTheme(value: string | null): void {
  if (value) draft.colorTheme = value as ColorTheme;
}

function updateColorMode(value: string | null): void {
  if (value) draft.colorMode = value as ColorMode;
}

function reset(): void {
  resetAppSettings();
  Object.assign(draft, appSettings);
}

async function chooseRecycleBinPath(): Promise<void> {
  if (!isTauri()) return;
  const selected = await open({ directory: true, multiple: false, title: "选择回收站文件夹" });
  if (typeof selected === "string") draft.recycleBinPath = selected;
}

async function openRecycleBin(): Promise<void> {
  recycleLocationError.value = "";
  try {
    await archiveRepository.openRecycleBin(draft.recycleBinPath || undefined);
  } catch (error) {
    recycleLocationError.value = error instanceof Error ? error.message : String(error);
  }
}

onMounted(() => {
  closeButton.value?.focus();
  void loadRecycleItems();
  void loadDiagnostics();
  void archiveRepository.getRepositoryInfo().then((info) => { repositoryPath.value = info.path; });
});

watch(activeSection, (section) => { if (section === "notifications") void loadDiagnostics(); });
</script>

<template>
  <div class="dialog-backdrop" @pointerdown="backdrop.pointerDown" @pointerup="backdrop.pointerUp" @pointercancel="backdrop.pointerCancel">
    <section class="settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <header>
        <div><p>CHRONICLE</p><h2 id="settings-title">设置</h2></div>
        <button ref="closeButton" class="close-button" aria-label="关闭设置" @click="emit('close')"><X :size="18" /></button>
      </header>

      <div class="settings-layout">
        <nav aria-label="设置分类">
          <button v-for="section in sections" :key="section.id" :class="{ active: activeSection === section.id }" @click="activeSection = section.id">
            <component :is="section.icon" :size="17" /><span>{{ section.label }}</span>
          </button>
        </nav>

        <main>
          <section v-if="activeSection === 'software'" aria-labelledby="software-title">
            <div class="section-heading"><h3 id="software-title">软件</h3><p>控制 Chronicle 的启动、关闭和通知行为。</p></div>
            <div class="setting-group">
              <label class="setting-row select-row"><span><b>配色主题</b><small>为 Chronicle 选择一组强调色。</small></span><ThemedSelect :model-value="draft.colorTheme" :options="colorThemeOptions" label="配色主题" @update:model-value="updateColorTheme" /></label>
              <label class="setting-row select-row"><span><b>显示模式</b><small>右上角太阳/月亮按钮可随时切换。</small></span><ThemedSelect :model-value="draft.colorMode" :options="colorModeOptions" label="显示模式" @update:model-value="updateColorMode" /></label>
              <label class="setting-row"><span><b>随系统启动</b><small>登录 Windows 后自动启动 Chronicle</small></span><input v-model="draft.launchAtStartup" type="checkbox" role="switch" /></label>
              <label class="setting-row"><span><b>自动检查更新</b><small>启动后检查稳定版本更新</small></span><input v-model="draft.checkForUpdates" type="checkbox" role="switch" /></label>
              <label class="setting-row"><span><b>桌面通知</b><small>备份、同步和恢复完成后显示通知</small></span><input v-model="draft.notifications" type="checkbox" role="switch" /></label>
              <label class="setting-row select-row"><span><b>关闭主窗口时</b><small>决定关闭按钮的默认行为</small></span><ThemedSelect :model-value="draft.closeBehavior" :options="closeBehaviorOptions" label="关闭主窗口时" @update:model-value="updateCloseBehavior" /></label>
            </div>
          </section>

          <section v-else-if="activeSection === 'notifications'" aria-labelledby="notifications-title">
            <div class="section-heading recycle-heading"><div><h3 id="notifications-title">通知与错误记录</h3><p>同步、备份和资料库操作失败时会保留脱敏后的详情。</p></div><div class="diagnostics-actions"><button :disabled="diagnosticsBusy || !diagnostics.length" @click="copyDiagnostics()"><ClipboardCopy :size="14" />复制全部</button><button class="empty-button" :disabled="diagnosticsBusy || !diagnostics.length" @click="clearDiagnostics"><Trash2 :size="14" />清空</button></div></div>
            <p v-if="diagnosticsError" class="recycle-error" role="alert">{{ diagnosticsError }}</p>
            <div v-if="diagnosticsBusy && !diagnostics.length" class="recycle-empty">正在读取错误记录…</div>
            <div v-else-if="!diagnostics.length" class="recycle-empty"><BellRing :size="24" /><b>暂无错误记录</b><span>后续失败操作会显示在这里。</span></div>
            <div v-else class="diagnostics-list"><article v-for="entry in diagnostics" :key="entry.id"><span><b>{{ entry.operation }}</b><small>{{ new Date(entry.occurredAt).toLocaleString('zh-CN') }} · {{ entry.message }}</small><code>{{ entry.details }}</code></span><button :aria-label="`复制 ${entry.operation} 错误详情`" @click="copyDiagnostics(entry)"><ClipboardCopy :size="15" />复制</button></article></div>
          </section>

          <section v-else-if="activeSection === 'backup'" aria-labelledby="backup-title">
            <div class="section-heading"><h3 id="backup-title">存储与备份</h3><p>设置新存档、自动备份和本地版本保留方式。</p></div>
            <div class="setting-group">
              <label class="setting-row"><span><b>立即创建首个备份</b><small>添加文件或文件夹后建立初始时间节点</small></span><input v-model="draft.createInitialSnapshot" type="checkbox" role="switch" /></label>
              <label class="setting-row select-row"><span><b>自动备份频率</b><small>仅在 Chronicle 运行时执行</small></span><ThemedSelect :model-value="draft.backupSchedule" :options="backupScheduleOptions" label="自动备份频率" @update:model-value="updateBackupSchedule" /></label>
              <div class="setting-row"><span><b>每个存档保留版本</b><small>默认保留全部版本；设置上限后清理最旧的普通备份</small></span><div class="retention-control"><label><input :checked="draft.retentionCount === null" type="checkbox" role="switch" @change="toggleRetentionLimit" /><span>无限制</span></label><input v-if="draft.retentionCount !== null" v-model.number="draft.retentionCount" aria-label="版本保留数量" class="number-input" type="number" min="1" max="999" /></div></div>
              <label class="setting-row"><span><b>启用回收站</b><small>删除的存档先移入回收站；关闭后直接永久删除</small></span><input v-model="draft.recycleBinEnabled" type="checkbox" role="switch" /></label>
              <div class="setting-row recycle-path"><span><b>回收站位置</b><small>点击路径可在资源管理器中打开；右侧按钮用于选择新的位置</small></span><div><button class="recycle-location" type="button" :title="recycleLocation" :disabled="!isTauri()" @click="openRecycleBin">{{ recycleLocation }}</button><button aria-label="选择回收站文件夹" :disabled="!isTauri()" @click="chooseRecycleBinPath"><FolderOpen :size="15" /></button></div></div>
              <p v-if="recycleLocationError" class="recycle-location-error" role="alert">{{ recycleLocationError }}</p>
            </div>
            <div class="path-card"><HardDrive :size="18" /><span><b>本地资料库</b><small>由 Chronicle 桌面应用数据目录管理</small></span><em>可用</em></div>
          </section>

          <section v-else-if="activeSection === 'recycle'" aria-labelledby="recycle-title">
            <div class="section-heading recycle-heading"><div><h3 id="recycle-title">回收站</h3><p>删除内容默认永久保留，可恢复或永久清理。</p></div><button class="empty-button" :disabled="recycleBusy || !recycleItems.length" @click="emptyRecycleBin"><Trash2 :size="14" />清空</button></div>
            <p v-if="recycleError" class="recycle-error" role="alert">{{ recycleError }}</p>
            <div v-if="recycleBusy && !recycleItems.length" class="recycle-empty">正在读取回收站…</div>
            <div v-else-if="!recycleItems.length" class="recycle-empty"><Trash2 :size="24" /><b>回收站为空</b><span>删除的存档和分类会显示在这里。</span></div>
            <div v-else class="recycle-list">
              <article v-for="item in recycleItems" :key="item.id">
                <span class="recycle-icon"><Trash2 :size="17" /></span><span><b>{{ item.displayName }}</b><small>{{ item.kind === 'category' ? '分类' : '存档' }} · {{ item.entryCount }} 个存档 · {{ formatBytes(item.sizeBytes) }} · {{ new Date(item.deletedAt).toLocaleString() }}</small></span>
                <div><button :disabled="recycleBusy" :aria-label="`恢复 ${item.displayName}`" @click="restoreItem(item)"><Undo2 :size="14" />恢复</button><button class="danger" :disabled="recycleBusy" :aria-label="`永久删除 ${item.displayName}`" @click="deleteItem(item)"><Trash2 :size="14" />删除</button></div>
              </article>
            </div>
          </section>

          <section v-else-if="activeSection === 'hotkeys'" aria-labelledby="hotkeys-title">
            <div class="section-heading"><h3 id="hotkeys-title">热键</h3><p>点击热键后按下组合键保存；Esc 取消，Delete 清除。</p></div>
            <div class="setting-group hotkey-group">
              <div class="setting-row"><span><b>聚焦搜索</b><small>在存档列表中开始查找</small></span><ShortcutRecorder v-model="draft.searchShortcut" label="聚焦搜索热键" /></div>
              <div class="setting-row"><span><b>创建备份</b><small>为当前选中的存档创建时间节点</small></span><ShortcutRecorder v-model="draft.snapshotShortcut" label="创建备份热键" /></div>
              <div class="setting-row"><span><b>打开设置</b><small>从任意主界面打开此窗口</small></span><ShortcutRecorder v-model="draft.settingsShortcut" label="打开设置热键" /></div>
            </div>
          </section>

          <section v-else aria-labelledby="about-title">
            <div class="section-heading"><h3 id="about-title">关于</h3><p>本地优先的通用文件时间节点管理器。</p></div>
            <div class="about-card"><img class="about-logo" :src="appIcon" alt="Chronicle 图标" /><div><h4>{{ appMetadata.name }}</h4><p>版本 {{ appMetadata.version }}</p><p>作者 {{ appMetadata.author }}</p></div></div>
            <dl class="about-list"><div><dt>存储引擎</dt><dd>Rust · 7z · SHA-256</dd></div><div><dt>桌面框架</dt><dd>Tauri 2 · Vue 3</dd></div><div><dt>许可证</dt><dd>尚未指定</dd></div></dl>
            <a href="https://github.com/ThermalEX/Chronicle" target="_blank" rel="noreferrer">查看 GitHub 仓库</a>
          </section>
        </main>
      </div>

      <footer><button class="reset-button" :disabled="saving" @click="reset"><RotateCcw :size="15" />恢复默认设置</button><div><button class="cancel-button" :disabled="saving" @click="emit('close')">取消</button><button class="save-button" :disabled="saving" @click="save">{{ saving ? '保存中' : '保存设置' }}</button></div></footer>
    </section>
    <ConfirmDialog v-if="confirmAction" :title="confirmAction.title" :message="confirmAction.message" confirm-label="确定" danger @cancel="confirmAction = undefined" @confirm="runRecycleAction" />
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 40; inset: 0; display: grid; place-items: center; padding: 32px; background: #18181b99; backdrop-filter: blur(3px); }
.settings-dialog { display: grid; grid-template-rows: 70px minmax(0, 1fr) 66px; width: min(860px, calc(100vw - 64px)); height: min(650px, calc(100vh - 64px)); overflow: hidden; background: var(--surface); border: 1px solid var(--border-2); border-radius: 13px; box-shadow: 0 24px 80px #0d24205c; }
header, footer { display: flex; align-items: center; justify-content: space-between; padding: 0 22px; }
header { border-bottom: 1px solid var(--border); }
header p { margin: 0; color: var(--primary); font-size: 9px; font-weight: 750; letter-spacing: .12em; }
header h2 { margin-top: 4px; font-size: 20px; }
.close-button { display: grid; place-items: center; width: 38px; height: 38px; background: transparent; border-radius: 7px; }
.close-button:hover, .cancel-button:hover, .reset-button:hover { background: var(--hover); }
.settings-layout { display: grid; grid-template-columns: 196px minmax(0, 1fr); min-height: 0; }
nav { padding: 15px 10px; background: var(--subtle); border-right: 1px solid var(--border); }
nav button { display: flex; align-items: center; gap: 10px; width: 100%; min-height: 42px; margin: 2px 0; padding: 0 12px; color: var(--text-2); background: transparent; border-radius: 7px; font-size: 12px; text-align: left; }
nav button:hover { color: var(--text); background: var(--hover); }
nav button.active { color: var(--primary-dark); background: var(--primary-soft); font-weight: 650; }
main { min-width: 0; overflow-y: auto; padding: 28px 32px 36px; }
.section-heading { margin-bottom: 20px; }
.section-heading h3 { font-size: 18px; }
.section-heading p { margin: 6px 0 0; color: var(--text-3); font-size: 11px; }
.setting-group { overflow: hidden; border: 1px solid var(--border); border-radius: 10px; }
.setting-row { display: flex; align-items: center; justify-content: space-between; min-height: 70px; gap: 28px; padding: 12px 16px; background: var(--surface); }
.setting-row + .setting-row { border-top: 1px solid var(--border); }
.setting-row > span { display: flex; min-width: 0; flex-direction: column; gap: 5px; }
.setting-row b { font-size: 12px; font-weight: 650; }
.setting-row small { color: var(--text-3); font-size: 10px; line-height: 1.4; }
.select-row :deep(.themed-select), .setting-row input[type="text"], .number-input { min-width: 152px; }
.setting-row input[type="checkbox"] { position: relative; width: 38px; height: 22px; flex: none; appearance: none; background: #cbd5d1; border-radius: 20px; cursor: pointer; transition: background .16s ease; }
.setting-row input[type="checkbox"]::after { content: ""; position: absolute; top: 3px; left: 3px; width: 16px; height: 16px; background: #fff; border-radius: 50%; box-shadow: 0 1px 3px #102b2738; transition: transform .16s ease; }
.setting-row input[type="checkbox"]:checked { background: var(--primary); }
.setting-row input[type="checkbox"]:checked::after { transform: translateX(16px); }
.number-input { min-width: 82px; width: 82px; }
.retention-control { display: flex; align-items: center; justify-content: flex-end; gap: 10px; }.retention-control label { display: flex; align-items: center; gap: 7px; color: var(--text-2); font-size: 10px; white-space: nowrap; }
.recycle-path > div { display: flex; align-items: center; gap: 7px; }.recycle-path input { width: 220px; }.recycle-path button { display: grid; place-items: center; width: 34px; height: 34px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 6px; }.recycle-path button:hover:not(:disabled) { background: #d2e5e0; }.recycle-path button:disabled { color: var(--text-3); cursor: default; opacity: .55; }
.path-card { display: grid; grid-template-columns: 22px 1fr auto; align-items: center; gap: 11px; margin-top: 16px; padding: 14px 16px; color: var(--primary); background: var(--primary-soft); border-radius: 9px; }
.path-card span { display: flex; flex-direction: column; gap: 3px; color: #263431; }
.path-card b { font-size: 11px; }.path-card small { color: var(--text-3); font-size: 9px; }.path-card em { color: var(--primary); font-size: 10px; font-style: normal; font-weight: 650; }
.about-card { display: flex; align-items: center; gap: 14px; padding: 18px; background: var(--subtle); border: 1px solid var(--border); border-radius: 10px; }
.about-logo { width: 48px; height: 48px; object-fit: cover; border-radius: 11px; }
.about-card h4, .about-card p { margin: 0; }.about-card h4 { font-size: 16px; }.about-card p { margin-top: 4px; color: var(--text-3); font-size: 10px; }
.about-list { margin: 18px 0; }.about-list div { display: flex; justify-content: space-between; padding: 11px 2px; border-bottom: 1px solid var(--border); font-size: 11px; }.about-list dt { color: var(--text-3); }.about-list dd { margin: 0; }
main a { color: var(--primary); font-size: 11px; font-weight: 650; text-decoration: none; }
footer { border-top: 1px solid var(--border); }
footer > div { display: flex; gap: 8px; }
footer button { min-height: 36px; padding: 0 13px; border-radius: 7px; font-size: 11px; font-weight: 650; }
.reset-button { display: inline-flex; align-items: center; gap: 7px; color: var(--text-2); background: transparent; }
.cancel-button { background: transparent; }.save-button { color: #fff; background: var(--primary); }.save-button:hover { background: var(--primary-dark); }
.recycle-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; }.recycle-heading h3 { margin: 0; }.empty-button { display: inline-flex; align-items: center; gap: 6px; min-height: 34px; padding: 0 10px; color: #a52e28; background: #fff0ef; border-radius: 7px; font-size: 10px; font-weight: 650; }.empty-button:disabled { color: var(--text-3); background: #f1f3f2; cursor: default; opacity: .65; }
.recycle-list { overflow: hidden; border: 1px solid var(--border); border-radius: 10px; }.recycle-list article { display: grid; grid-template-columns: 36px minmax(0, 1fr) auto; align-items: center; gap: 10px; min-height: 66px; padding: 9px 12px; }.recycle-list article + article { border-top: 1px solid var(--border); }.recycle-icon { display: grid; place-items: center; width: 32px; height: 32px; color: #a52e28; background: #fff0ef; border-radius: 7px; }.recycle-list article > span:nth-child(2) { display: flex; min-width: 0; flex-direction: column; gap: 4px; }.recycle-list b { font-size: 11px; }.recycle-list small { overflow: hidden; color: var(--text-3); font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }.recycle-list article > div { display: flex; gap: 6px; }.recycle-list button { display: inline-flex; align-items: center; gap: 5px; min-height: 32px; padding: 0 8px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 6px; font-size: 9px; }.recycle-list button.danger { color: #a52e28; background: #fff0ef; }.recycle-list button:disabled { cursor: default; opacity: .55; }
.recycle-empty { display: grid; place-items: center; min-height: 210px; gap: 7px; color: var(--text-3); background: #f8faf9; border: 1px dashed var(--border-2); border-radius: 10px; font-size: 10px; }.recycle-empty b { color: var(--text-2); font-size: 12px; }.recycle-error { padding: 10px 12px; color: #a52e28; background: #fff0ef; border-radius: 7px; font-size: 10px; }
.diagnostics-actions { display: flex; gap: 7px; }.diagnostics-actions button { display: inline-flex; align-items: center; gap: 5px; min-height: 32px; padding: 0 9px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 7px; font-size: 10px; font-weight: 650; }.diagnostics-actions .empty-button { color: #a52e28; background: #fff0ef; }.diagnostics-list { overflow: hidden; border: 1px solid var(--border); border-radius: 9px; }.diagnostics-list article { display: flex; align-items: start; justify-content: space-between; gap: 12px; padding: 12px; }.diagnostics-list article + article { border-top: 1px solid var(--border); }.diagnostics-list article > span { display: grid; min-width: 0; gap: 4px; }.diagnostics-list b { color: var(--text); font-size: 11px; }.diagnostics-list small { overflow: hidden; color: var(--text-3); font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }.diagnostics-list code { overflow: auto; max-height: 88px; padding: 7px; color: var(--text-2); background: #f6f8f7; border-radius: 5px; font-family: ui-monospace, Consolas, monospace; font-size: 9px; line-height: 1.45; white-space: pre-wrap; }.diagnostics-list article > button { display: inline-flex; flex: 0 0 auto; align-items: center; gap: 5px; min-height: 30px; padding: 0 8px; color: var(--text-2); background: #f2f6f4; border-radius: 6px; font-size: 9px; }
.recycle-path { align-items: stretch; gap: 20px; }.recycle-path > span { flex: 0 1 34%; }.recycle-path > div { display: flex; flex: 1 1 auto; align-items: stretch; min-width: 0; gap: 7px; }.recycle-location { flex: 1 1 auto; min-width: 0; min-height: 46px; padding: 8px 10px; color: var(--text-2); background: #f8faf9; border: 1px solid var(--border-2); border-radius: 6px; font-size: 10px; line-height: 1.45; text-align: left; overflow-wrap: anywhere; white-space: normal; }.recycle-location:hover:not(:disabled) { color: var(--primary-dark); border-color: #8bbdb4; background: #fff; }.recycle-path > div > button:last-child { display: grid; flex: 0 0 34px; place-items: center; width: 34px; min-height: 46px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 6px; }.recycle-path > div > button:last-child:hover:not(:disabled) { background: #d2e5e0; }.recycle-path button:disabled { color: var(--text-3); cursor: default; opacity: .55; }.recycle-location-error { margin: -3px 16px 10px; color: #a52e28; font-size: 9px; }@media (max-width: 1100px) { .settings-dialog { width: calc(100vw - 40px); height: calc(100vh - 40px); }.dialog-backdrop { padding: 20px; }.recycle-path { align-items: flex-start; flex-direction: column; }.recycle-path > span { flex-basis: auto; }.recycle-path > div { width: 100%; } }
/* Appearance controls use the same semantic surfaces as the application. */
.setting-row input[type="checkbox"] { background: var(--border-2); }
.setting-row input[type="checkbox"]::after { background: var(--surface); }
.recycle-path button:hover:not(:disabled), .recycle-path > div > button:last-child:hover:not(:disabled) { background: var(--hover); }
.path-card span { color: var(--text); }
.save-button { color: var(--on-primary); }
.empty-button, .diagnostics-actions .empty-button, .recycle-list button.danger, .recycle-icon, .recycle-error { color: var(--danger); background: var(--danger-soft); }
.recycle-empty, .diagnostics-list code, .recycle-location { background: var(--subtle); }
.recycle-location:hover:not(:disabled) { border-color: var(--primary); background: var(--surface-raised); }
</style>
