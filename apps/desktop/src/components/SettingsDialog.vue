<script setup lang="ts">
import { HardDrive, Info, Keyboard, RotateCcw, Settings2, X } from "@lucide/vue";
import { onMounted, reactive, ref } from "vue";
import { appSettings, resetAppSettings, saveAppSettings, type AppSettings } from "../services/settings";

const emit = defineEmits<{ close: []; saved: [] }>();
const activeSection = ref<"software" | "backup" | "hotkeys" | "about">("software");
const closeButton = ref<HTMLButtonElement>();
const draft = reactive<AppSettings>({ ...appSettings });
const saving = ref(false);

const sections = [
  { id: "software" as const, label: "软件", icon: Settings2 },
  { id: "backup" as const, label: "存储与备份", icon: HardDrive },
  { id: "hotkeys" as const, label: "热键", icon: Keyboard },
  { id: "about" as const, label: "关于", icon: Info },
];

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

function reset(): void {
  resetAppSettings();
  Object.assign(draft, appSettings);
}

onMounted(() => closeButton.value?.focus());
</script>

<template>
  <div class="dialog-backdrop" @click.self="emit('close')">
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
              <label class="setting-row"><span><b>随系统启动</b><small>登录 Windows 后自动启动 Chronicle</small></span><input v-model="draft.launchAtStartup" type="checkbox" role="switch" /></label>
              <label class="setting-row"><span><b>自动检查更新</b><small>启动后检查稳定版本更新</small></span><input v-model="draft.checkForUpdates" type="checkbox" role="switch" /></label>
              <label class="setting-row"><span><b>桌面通知</b><small>备份、同步和恢复完成后显示通知</small></span><input v-model="draft.notifications" type="checkbox" role="switch" /></label>
              <label class="setting-row select-row"><span><b>关闭主窗口时</b><small>决定关闭按钮的默认行为</small></span><select v-model="draft.closeBehavior"><option value="ask">每次询问</option><option value="tray">最小化到托盘</option><option value="exit">退出 Chronicle</option></select></label>
            </div>
          </section>

          <section v-else-if="activeSection === 'backup'" aria-labelledby="backup-title">
            <div class="section-heading"><h3 id="backup-title">存储与备份</h3><p>设置新存档、自动备份和本地版本保留方式。</p></div>
            <div class="setting-group">
              <label class="setting-row"><span><b>立即创建首个备份</b><small>添加文件或文件夹后建立初始时间节点</small></span><input v-model="draft.createInitialSnapshot" type="checkbox" role="switch" /></label>
              <label class="setting-row select-row"><span><b>自动备份频率</b><small>仅在 Chronicle 运行时执行</small></span><select v-model="draft.backupSchedule"><option value="off">关闭</option><option value="15m">每 15 分钟</option><option value="1h">每小时</option><option value="6h">每 6 小时</option><option value="daily">每天</option></select></label>
              <div class="setting-row"><span><b>每个存档保留版本</b><small>默认保留全部版本；设置上限后清理最旧的普通备份</small></span><div class="retention-control"><label><input :checked="draft.retentionCount === null" type="checkbox" role="switch" @change="toggleRetentionLimit" /><span>无限制</span></label><input v-if="draft.retentionCount !== null" v-model.number="draft.retentionCount" aria-label="版本保留数量" class="number-input" type="number" min="1" max="999" /></div></div>
            </div>
            <div class="path-card"><HardDrive :size="18" /><span><b>本地资料库</b><small>由 Chronicle 桌面应用数据目录管理</small></span><em>可用</em></div>
          </section>

          <section v-else-if="activeSection === 'hotkeys'" aria-labelledby="hotkeys-title">
            <div class="section-heading"><h3 id="hotkeys-title">热键</h3><p>使用 Ctrl、Shift、Alt 和一个按键组合。</p></div>
            <div class="setting-group hotkey-group">
              <label class="setting-row"><span><b>聚焦搜索</b><small>在存档列表中开始查找</small></span><input v-model.trim="draft.searchShortcut" type="text" aria-label="聚焦搜索热键" /></label>
              <label class="setting-row"><span><b>创建备份</b><small>为当前选中的存档创建时间节点</small></span><input v-model.trim="draft.snapshotShortcut" type="text" aria-label="创建备份热键" /></label>
              <label class="setting-row"><span><b>打开设置</b><small>从任意主界面打开此窗口</small></span><input v-model.trim="draft.settingsShortcut" type="text" aria-label="打开设置热键" /></label>
            </div>
          </section>

          <section v-else aria-labelledby="about-title">
            <div class="section-heading"><h3 id="about-title">关于</h3><p>本地优先的通用文件时间节点管理器。</p></div>
            <div class="about-card"><span class="about-logo"><Settings2 :size="24" /></span><div><h4>Chronicle</h4><p>版本 0.1.0</p></div></div>
            <dl class="about-list"><div><dt>存储引擎</dt><dd>Rust · 7z · SHA-256</dd></div><div><dt>桌面框架</dt><dd>Tauri 2 · Vue 3</dd></div><div><dt>许可证</dt><dd>尚未指定</dd></div></dl>
            <a href="https://github.com/ThermalEX/Chronicle" target="_blank" rel="noreferrer">查看 GitHub 仓库</a>
          </section>
        </main>
      </div>

      <footer><button class="reset-button" :disabled="saving" @click="reset"><RotateCcw :size="15" />恢复默认设置</button><div><button class="cancel-button" :disabled="saving" @click="emit('close')">取消</button><button class="save-button" :disabled="saving" @click="save">{{ saving ? '保存中' : '保存设置' }}</button></div></footer>
    </section>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 40; inset: 0; display: grid; place-items: center; padding: 32px; background: #1024218a; backdrop-filter: blur(3px); }
.settings-dialog { display: grid; grid-template-rows: 70px minmax(0, 1fr) 66px; width: min(860px, calc(100vw - 64px)); height: min(650px, calc(100vh - 64px)); overflow: hidden; background: var(--surface); border: 1px solid var(--border-2); border-radius: 13px; box-shadow: 0 24px 80px #0d24205c; }
header, footer { display: flex; align-items: center; justify-content: space-between; padding: 0 22px; }
header { border-bottom: 1px solid var(--border); }
header p { margin: 0; color: var(--primary); font-size: 9px; font-weight: 750; letter-spacing: .12em; }
header h2 { margin-top: 4px; font-size: 20px; }
.close-button { display: grid; place-items: center; width: 38px; height: 38px; background: transparent; border-radius: 7px; }
.close-button:hover, .cancel-button:hover, .reset-button:hover { background: var(--hover); }
.settings-layout { display: grid; grid-template-columns: 196px minmax(0, 1fr); min-height: 0; }
nav { padding: 15px 10px; background: #f1f5f3; border-right: 1px solid var(--border); }
nav button { display: flex; align-items: center; gap: 10px; width: 100%; min-height: 42px; margin: 2px 0; padding: 0 12px; color: var(--text-2); background: transparent; border-radius: 7px; font-size: 12px; text-align: left; }
nav button:hover { color: #17211f; background: #e5ebe8; }
nav button.active { color: var(--primary-dark); background: #dcece8; font-weight: 650; }
main { min-width: 0; overflow-y: auto; padding: 28px 32px 36px; }
.section-heading { margin-bottom: 20px; }
.section-heading h3 { font-size: 18px; }
.section-heading p { margin: 6px 0 0; color: var(--text-3); font-size: 11px; }
.setting-group { overflow: hidden; border: 1px solid var(--border); border-radius: 10px; }
.setting-row { display: flex; align-items: center; justify-content: space-between; min-height: 70px; gap: 28px; padding: 12px 16px; background: #fff; }
.setting-row + .setting-row { border-top: 1px solid var(--border); }
.setting-row > span { display: flex; min-width: 0; flex-direction: column; gap: 5px; }
.setting-row b { font-size: 12px; font-weight: 650; }
.setting-row small { color: var(--text-3); font-size: 10px; line-height: 1.4; }
select, .setting-row input[type="text"], .number-input { min-width: 152px; height: 34px; padding: 0 9px; color: #263431; background: #f8faf9; border: 1px solid var(--border-2); border-radius: 6px; font-size: 11px; }
.setting-row input[type="checkbox"] { position: relative; width: 38px; height: 22px; flex: none; appearance: none; background: #cbd5d1; border-radius: 20px; cursor: pointer; transition: background .16s ease; }
.setting-row input[type="checkbox"]::after { content: ""; position: absolute; top: 3px; left: 3px; width: 16px; height: 16px; background: #fff; border-radius: 50%; box-shadow: 0 1px 3px #102b2738; transition: transform .16s ease; }
.setting-row input[type="checkbox"]:checked { background: var(--primary); }
.setting-row input[type="checkbox"]:checked::after { transform: translateX(16px); }
.number-input { min-width: 82px; width: 82px; }
.retention-control { display: flex; align-items: center; justify-content: flex-end; gap: 10px; }.retention-control label { display: flex; align-items: center; gap: 7px; color: var(--text-2); font-size: 10px; white-space: nowrap; }
.hotkey-group input { width: 154px; font-family: "Cascadia Code", Consolas, monospace; text-align: center; }
.path-card { display: grid; grid-template-columns: 22px 1fr auto; align-items: center; gap: 11px; margin-top: 16px; padding: 14px 16px; color: var(--primary); background: var(--primary-soft); border-radius: 9px; }
.path-card span { display: flex; flex-direction: column; gap: 3px; color: #263431; }
.path-card b { font-size: 11px; }.path-card small { color: var(--text-3); font-size: 9px; }.path-card em { color: var(--primary); font-size: 10px; font-style: normal; font-weight: 650; }
.about-card { display: flex; align-items: center; gap: 14px; padding: 18px; background: #f4f8f6; border: 1px solid var(--border); border-radius: 10px; }
.about-logo { display: grid; place-items: center; width: 48px; height: 48px; color: #fff; background: #153b37; border-radius: 11px; }
.about-card h4, .about-card p { margin: 0; }.about-card h4 { font-size: 16px; }.about-card p { margin-top: 4px; color: var(--text-3); font-size: 10px; }
.about-list { margin: 18px 0; }.about-list div { display: flex; justify-content: space-between; padding: 11px 2px; border-bottom: 1px solid var(--border); font-size: 11px; }.about-list dt { color: var(--text-3); }.about-list dd { margin: 0; }
main a { color: var(--primary); font-size: 11px; font-weight: 650; text-decoration: none; }
footer { border-top: 1px solid var(--border); }
footer > div { display: flex; gap: 8px; }
footer button { min-height: 36px; padding: 0 13px; border-radius: 7px; font-size: 11px; font-weight: 650; }
.reset-button { display: inline-flex; align-items: center; gap: 7px; color: var(--text-2); background: transparent; }
.cancel-button { background: transparent; }.save-button { color: #fff; background: var(--primary); }.save-button:hover { background: var(--primary-dark); }
@media (max-width: 1100px) { .settings-dialog { width: calc(100vw - 40px); height: calc(100vh - 40px); }.dialog-backdrop { padding: 20px; } }
</style>
