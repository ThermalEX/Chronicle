<script setup lang="ts">
import { CheckCircle2, CloudCog, LockKeyhole, X } from "@lucide/vue";
import { onMounted, reactive, ref } from "vue";
import { cloudSettings, saveCloudSettings, type CloudSettings } from "../services/settings";
import ThemedSelect, { type ThemedSelectOption } from "./ThemedSelect.vue";

const emit = defineEmits<{ close: []; saved: [] }>();
const closeButton = ref<HTMLButtonElement>();
const draft = reactive<any>({ ...cloudSettings });
const password = ref("");
const saving = ref(false);
const directionOptions: ThemedSelectOption[] = [
  { value: "bidirectional", label: "双向同步" },
  { value: "upload", label: "仅上传" },
  { value: "download", label: "仅下载" },
];
const conflictOptions: ThemedSelectOption[] = [
  { value: "ask", label: "每次询问" },
  { value: "newest", label: "保留较新版本" },
  { value: "local", label: "优先本地" },
  { value: "remote", label: "优先远端" },
];

async function save(): Promise<void> {
  draft.maxConcurrentMetadataReads = Math.max(1, Math.min(4, Number(draft.maxConcurrentMetadataReads) || 2));
  draft.maxConcurrentTransfers = Math.max(1, Math.min(4, Number(draft.maxConcurrentTransfers) || 2));
  draft.requestDelayMs = Math.max(0, Math.min(5000, Number(draft.requestDelayMs) || 0));
  draft.retryLimit = Math.max(1, Math.min(10, Number(draft.retryLimit) || 5));
  saving.value = true;
  try {
    await saveCloudSettings({ ...draft });
    emit("saved");
    emit("close");
  } finally {
    saving.value = false;
  }
}

onMounted(() => closeButton.value?.focus());
</script>

<template>
  <div class="dialog-backdrop" @click.self="emit('close')">
    <section class="cloud-dialog" role="dialog" aria-modal="true" aria-labelledby="cloud-title">
      <header><div class="heading-icon"><CloudCog :size="21" /></div><div><p>同步服务</p><h2 id="cloud-title">云端设置</h2></div><button ref="closeButton" aria-label="关闭云端设置" @click="emit('close')"><X :size="18" /></button></header>
      <main>
        <label class="enable-row"><span><b>启用 WebDAV 同步</b><small>在本地资料库和远端目录之间同步快照</small></span><input v-model="draft.enabled" type="checkbox" role="switch" /></label>

        <fieldset :disabled="!draft.enabled">
          <legend>服务器</legend>
          <label><span>服务器地址</span><input v-model.trim="draft.endpoint" type="url" placeholder="https://dav.example.com/remote.php/dav/files/user" /></label>
          <div class="field-grid"><label><span>用户名</span><input v-model.trim="draft.username" autocomplete="username" type="text" /></label><label><span>密码</span><input v-model="password" autocomplete="current-password" type="password" /></label></div>
          <label><span>远端目录</span><input v-model.trim="draft.remotePath" type="text" placeholder="/Chronicle" /></label>
          <p class="security-note"><LockKeyhole :size="14" />密码仅保留在当前会话；凭据存储接入系统保险库后再长期保存。</p>
        </fieldset>

        <fieldset :disabled="!draft.enabled">
          <legend>同步策略</legend>
          <div class="field-grid"><label><span>同步方向</span><ThemedSelect v-model="draft.syncDirection" :options="directionOptions" label="同步方向" /></label><label><span>冲突处理</span><ThemedSelect v-model="draft.conflictStrategy" :options="conflictOptions" label="冲突处理" /></label></div>
          <label class="inline-toggle"><span><b>启动时同步</b><small>打开 Chronicle 后检查远端变化</small></span><input v-model="draft.syncOnLaunch" type="checkbox" role="switch" /></label>
        </fieldset>

        <fieldset :disabled="!draft.enabled">
          <legend>请求控制</legend>
          <div class="field-grid"><label><span>同时读取元数据</span><input v-model.number="draft.maxConcurrentMetadataReads" type="number" min="1" max="4" /></label><label><span>同时上传或下载</span><input v-model.number="draft.maxConcurrentTransfers" type="number" min="1" max="4" /></label></div>
          <div class="field-grid"><label><span>请求间隔（毫秒）</span><input v-model.number="draft.requestDelayMs" type="number" min="0" max="5000" step="50" /></label><label><span>临时错误重试次数</span><input v-model.number="draft.retryLimit" type="number" min="1" max="10" /></label></div>
          <p class="security-note">默认最多同时执行 2 个传输；遇到 429、502、503 或 504 时按服务器要求延迟并指数退避。</p>
        </fieldset>

        <div class="connection-state"><CheckCircle2 :size="16" /><span><b>配置保存在本机</b><small>WebDAV 连接和同步引擎将在云端模块接入后启用</small></span></div>
      </main>
      <footer><button class="cancel-button" :disabled="saving" @click="emit('close')">取消</button><button class="save-button" :disabled="saving" @click="save">{{ saving ? '保存中' : '保存云端设置' }}</button></footer>
    </section>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 45; inset: 0; display: grid; place-items: center; padding: 32px; background: #1024218a; backdrop-filter: blur(3px); }
.cloud-dialog { display: grid; grid-template-rows: 76px minmax(0, 1fr) 66px; width: min(660px, calc(100vw - 64px)); max-height: calc(100vh - 64px); overflow: hidden; background: var(--surface); border: 1px solid var(--border-2); border-radius: 13px; box-shadow: 0 24px 80px #0d24205c; }
header { display: grid; grid-template-columns: 42px 1fr 38px; align-items: center; gap: 11px; padding: 0 20px; border-bottom: 1px solid var(--border); }
.heading-icon { display: grid; place-items: center; width: 38px; height: 38px; color: var(--primary); background: var(--primary-soft); border-radius: 9px; }
header p, header h2 { margin: 0; }header p { color: var(--text-3); font-size: 9px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }header h2 { margin-top: 3px; font-size: 18px; }
header button { display: grid; place-items: center; width: 38px; height: 38px; background: transparent; border-radius: 7px; }header button:hover { background: var(--hover); }
main { overflow-y: auto; padding: 22px 26px 28px; }
.enable-row, .inline-toggle { display: flex; align-items: center; justify-content: space-between; gap: 24px; }
.enable-row { min-height: 68px; padding: 12px 15px; background: var(--primary-soft); border: 1px solid #bddfd8; border-radius: 9px; }
.enable-row span, .inline-toggle span, .connection-state span { display: flex; flex-direction: column; gap: 4px; }.enable-row b, .inline-toggle b, .connection-state b { font-size: 11px; }.enable-row small, .inline-toggle small, .connection-state small { color: var(--text-3); font-size: 9px; line-height: 1.4; }
input[type="checkbox"] { position: relative; width: 38px; height: 22px; flex: none; appearance: none; background: #cbd5d1; border-radius: 20px; cursor: pointer; }input[type="checkbox"]::after { content: ""; position: absolute; top: 3px; left: 3px; width: 16px; height: 16px; background: #fff; border-radius: 50%; box-shadow: 0 1px 3px #102b2738; transition: transform .16s ease; }input[type="checkbox"]:checked { background: var(--primary); }input[type="checkbox"]:checked::after { transform: translateX(16px); }
fieldset { display: grid; gap: 14px; margin: 20px 0 0; padding: 17px; border: 1px solid var(--border); border-radius: 9px; }fieldset:disabled { opacity: .52; }legend { padding: 0 6px; color: var(--text-2); font-size: 10px; font-weight: 700; }
label { display: flex; flex-direction: column; gap: 6px; }label > span { color: var(--text-2); font-size: 10px; font-weight: 650; }
input[type="text"], input[type="url"], input[type="password"], input[type="number"] { width: 100%; height: 36px; padding: 0 10px; color: #263431; background: #f8faf9; border: 1px solid var(--border-2); border-radius: 6px; font-size: 11px; }
.field-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }.inline-toggle { flex-direction: row; min-height: 48px; }.security-note { display: flex; align-items: center; gap: 7px; margin: 0; color: var(--text-3); font-size: 9px; }
.field-grid :deep(.trigger) { height: 36px; font-size: 11px; }
.connection-state { display: grid; grid-template-columns: 18px 1fr; align-items: center; gap: 9px; margin-top: 17px; padding: 11px 13px; color: var(--primary); background: #f3f8f6; border-radius: 7px; }
footer { display: flex; align-items: center; justify-content: flex-end; gap: 8px; padding: 0 20px; border-top: 1px solid var(--border); }footer button { min-height: 36px; padding: 0 13px; border-radius: 7px; font-size: 11px; font-weight: 650; }.cancel-button { background: transparent; }.cancel-button:hover { background: var(--hover); }.save-button { color: #fff; background: var(--primary); }.save-button:hover { background: var(--primary-dark); }
@media (max-width: 800px) { .field-grid { grid-template-columns: 1fr; } }
</style>
