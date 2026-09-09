<script setup lang="ts">
import { CheckCircle2, ChevronDown, CircleAlert, CloudCog, Download, ExternalLink, FileJson, Pause, Play, Plus, RefreshCw, Search, Server, Trash2, Upload, X } from "@lucide/vue";
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { cloudRepository, saveCloudConfiguration, type CloudPreview, type RemoteItem } from "../services/cloud";
import { cloudSettings, type CloudSettings, type CloudSource } from "../services/settings";
import { createBackdropDismissal } from "../services/dialogDismissal";
import ConfirmDialog from "./ConfirmDialog.vue";
import ThemedSelect, { type ThemedSelectOption } from "./ThemedSelect.vue";
import { openExternalUrl } from "../services/externalBrowser";
import { githubClassicPatUrl, githubRepositoryName } from "../services/githubPat";
import OpenDalSourceFields from "./OpenDalSourceFields.vue";
import { configureOpenDal, secretPatch, sourceTestKey, validateOpenDal } from "../services/opendal";
import { initialExpandedSourceIds, toggleExpandedSource } from "../services/sourceCardState";
import { newCloudSource, toggleSourceSync } from "../services/cloudSourceControls";

const emit = defineEmits<{ close: []; saved: [] }>();
const tab = ref<"repository" | "sources">("repository");
const draft = reactive<CloudSettings>({ ...cloudSettings, sources: cloudSettings.sources.map((source) => ({ ...source, ...(source.config ? { config: { ...source.config } } : {}), ...(source.secretKeys ? { secretKeys: [...source.secretKeys] } : {}) })) });
const passwords = reactive<Record<string, string>>({});
const secrets = reactive<Record<string, Record<string, string>>>(Object.fromEntries(draft.sources.map((source) => [source.id, {}])));
const tested = reactive<Record<string, string>>({});
const failedTests = reactive(new Set<string>());
const savedCredentials = reactive<Record<string, boolean>>({});
const original = new Map(draft.sources.map((source) => [source.id, sourceTestKey(source, {})]));
function currentTestKey(source: CloudSource): string { return sourceTestKey(source, secretPatch(secrets[source.id] ?? {})); }
function requiresTest(source: CloudSource): boolean {
  return source.provider === "opendal" && (failedTests.has(source.id) || (currentTestKey(source) !== original.get(source.id) && tested[source.id] !== currentTestKey(source)));
}
const newRepositoryNames = reactive<Record<string, string>>({});
const closeButton = ref<HTMLButtonElement>();
const preview = ref<CloudPreview>();
const selected = ref<string[]>([]);
const busy = ref("");
const backdrop = createBackdropDismissal(() => emit("close"), () => !busy.value);
const feedback = ref("");
const error = ref("");
const toast = ref<{ type: "success" | "error"; message: string }>();
const expandedSourceIds = ref(initialExpandedSourceIds());
let toastTimer: number | undefined;
const confirmAction = ref<{ title: string; message: string; run: () => Promise<void> }>();
const repositorySourceId = ref<string | null>(draft.sources[0]?.id ?? null);
const repositorySource = computed(() => draft.sources.find((source) => source.id === repositorySourceId.value));
const cloudSearch = ref("");
const remoteArchives = computed(() => (preview.value?.items ?? []).filter((item) => item.kind === "archive"));
const visibleRemoteArchives = computed(() => {
  const query = cloudSearch.value.trim().toLocaleLowerCase();
  if (!query) return remoteArchives.value;
  return remoteArchives.value.filter((item) => item.name.toLocaleLowerCase().includes(query));
});
const remoteSnapshotTotal = computed(() => remoteArchives.value.reduce((total, item) => total + item.snapshotCount, 0));
function showToast(type: "success" | "error", message: string): void {
  toast.value = { type, message };
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => { toast.value = undefined; }, type === "success" ? 3600 : 7000);
}
watch(feedback, (message) => { if (message) showToast("success", message); });
watch(error, (message) => { if (message) showToast("error", message); });
const sourceOptions = computed<ThemedSelectOption[]>(() => [
  { value: null, label: "未选择" },
  ...draft.sources.map((source) => ({ value: source.id, label: source.name })),
]);
const syncOptions: ThemedSelectOption[] = [
  { value: "manual", label: "手动同步" },
  { value: "automatic", label: "自动上传" },
];

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

function formatUpdatedAt(value?: number): string {
  if (!value) return "未记录";
  return new Intl.DateTimeFormat("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
}

function addSource(): void {
  const id = crypto.randomUUID();
  draft.sources.push(newCloudSource("legacy_webdav", id, draft.sources.length + 1));
  expandedSourceIds.value = initialExpandedSourceIds();
  tab.value = "sources";
}

function addGitHubSource(): void {
  const id = crypto.randomUUID();
  draft.sources.push(newCloudSource("legacy_github", id, draft.sources.length + 1));
  expandedSourceIds.value = initialExpandedSourceIds();
  tab.value = "sources";
}

function addOpenDalSource(): void {
  const id = crypto.randomUUID();
  const source: CloudSource = newCloudSource("opendal", id, draft.sources.length + 1);
  configureOpenDal(source, "s3");
  secrets[id] = {};
  draft.sources.push(source);
  expandedSourceIds.value = initialExpandedSourceIds();
  tab.value = "sources";
}

async function persist(): Promise<void> {
  draft.enabled = Boolean(draft.sources.length);
  for (const source of draft.sources) {
    if (source.provider === "opendal") {
      const problem = validateOpenDal(source);
      if (problem) throw new Error(`${source.name}：${problem}`);
      if (requiresTest(source)) throw new Error(`“${source.name}”配置已更改，请先测试读写、列举和清理`);
    }
  }
  // Credential persistence must succeed before settings can enable a new source.
  const credentials = draft.sources.filter((source) => source.provider === "opendal"
    ? currentTestKey(source) !== original.get(source.id) : Boolean(passwords[source.id]))
    .map((source) => ({ source, password: passwords[source.id], secrets: secretPatch(secrets[source.id] ?? {}) }));
  await saveCloudConfiguration({ ...draft, sources: draft.sources.map((source) => ({ ...source })) }, credentials);
  for (const source of draft.sources) {
    secrets[source.id] = {};
    passwords[source.id] = "";
    original.set(source.id, sourceTestKey(source, {}));
  }
  await loadSourceStatuses();
  emit("saved");
}

async function loadSourceStatuses(): Promise<void> {
  try {
    const statuses = await cloudRepository.sourceStatuses();
    for (const source of draft.sources) savedCredentials[source.id] = statuses.find((status) => status.sourceId === source.id)?.credentialSaved ?? false;
  } catch {
    for (const source of draft.sources) savedCredentials[source.id] = false;
  }
}

async function loadPreview(): Promise<void> {
  if (!repositorySource.value) return;
  busy.value = "preview"; error.value = ""; feedback.value = "";
  try { preview.value = await cloudRepository.preview(repositorySource.value.id); selected.value = []; }
  catch (reason) { error.value = reason instanceof Error ? reason.message : String(reason); }
  finally { busy.value = ""; }
}

async function testSource(source: CloudSource): Promise<void> {
  busy.value = `test:${source.id}`; error.value = ""; feedback.value = "";
  delete tested[source.id];
  failedTests.add(source.id);
  try {
    if (source.provider === "opendal") {
      const problem = validateOpenDal(source);
      if (problem) throw new Error(problem);
    }
    const key = currentTestKey(source);
    await cloudRepository.test(source, source.provider === "opendal" ? JSON.stringify(secretPatch(secrets[source.id] ?? {})) : passwords[source.id] ?? "");
    tested[source.id] = key;
    failedTests.delete(source.id);
    feedback.value = source.provider === "opendal" ? `“${source.name}”读写、列举和临时清理测试通过` : `“${source.name}”连接与读写测试通过`;
  }
  catch (reason) { error.value = reason instanceof Error ? reason.message : String(reason); }
  finally { busy.value = ""; }
}

async function createGitHubRepository(source: CloudSource): Promise<void> {
  const repositoryName = newRepositoryNames[source.id]?.trim() || githubRepositoryName(source.id);
  busy.value = `create-repository:${source.id}`; error.value = ""; feedback.value = "";
  try {
    const created = await cloudRepository.createGitHubRepository(source, repositoryName, passwords[source.id] ?? "");
    source.repository = created.repository;
    source.branch = created.branch;
    await persist();
    feedback.value = `已创建私有仓库“${created.repository}”，默认分支为 ${created.branch}`;
  } catch (reason) { error.value = reason instanceof Error ? reason.message : String(reason); }
  finally { busy.value = ""; }
}

async function openGitHubPatPage(): Promise<void> {
  error.value = "";
  try {
    await openExternalUrl(githubClassicPatUrl());
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason);
  }
}

function removeSource(source: CloudSource): void {
  confirmAction.value = { title: "删除同步源", message: `删除“${source.name}”的本机配置，远端文件不会被删除。`, run: async () => {
    draft.sources = draft.sources.filter((item) => item.id !== source.id);
    if (repositorySourceId.value === source.id) repositorySourceId.value = draft.sources[0]?.id ?? null;
    await persist(); preview.value = undefined;
  } };
}

async function runItemAction(item: RemoteItem, action: "sync" | "upload" | "download"): Promise<void> {
  if (!repositorySource.value || item.protected) return;
  busy.value = `${action}:${item.id}`; error.value = ""; feedback.value = "";
  try {
    if (action === "sync") feedback.value = (await cloudRepository.sync(repositorySource.value.id, item.id)).message;
    if (action === "upload") { await cloudRepository.upload(repositorySource.value.id, item.id); feedback.value = "已用本地存档覆盖远端"; }
    if (action === "download") { await cloudRepository.download(repositorySource.value.id, item.id); feedback.value = "已用远端存档覆盖本地仓库，来源文件未改动"; }
    await loadPreview();
  } catch (reason) { error.value = reason instanceof Error ? reason.message : String(reason); }
  finally { busy.value = ""; }
}

function confirmOverwrite(item: RemoteItem, direction: "upload" | "download"): void {
  confirmAction.value = {
    title: direction === "upload" ? "覆盖上传" : "覆盖下载",
    message: direction === "upload" ? `远端“${item.name}”将被本地仓库完整替换。` : `本地仓库中的“${item.name}”将被远端版本完整替换，原始来源文件不会改动。`,
    run: () => runItemAction(item, direction),
  };
}

function confirmDelete(ids: string[]): void {
  const deletable = ids.filter((id) => remoteArchives.value.some((item) => item.id === id));
  if (!deletable.length || !repositorySource.value) return;
  confirmAction.value = { title: "删除云端存档", message: `${deletable.length} 个云端存档及其全部时间节点将永久删除。`, run: async () => {
    await cloudRepository.delete(repositorySource.value!.id, deletable); await loadPreview();
  } };
}

async function changeSyncMode(item: RemoteItem, mode: string | null): Promise<void> {
  if (!repositorySource.value || (mode !== "manual" && mode !== "automatic")) return;
  busy.value = `mode:${item.id}`;
  try { await cloudRepository.setSyncMode(repositorySource.value.id, item.id, mode); item.syncMode = mode; feedback.value = "同步方案已保存"; }
  catch (reason) { error.value = reason instanceof Error ? reason.message : String(reason); }
  finally { busy.value = ""; }
}

function selectRepositorySource(sourceId: string | null): void {
  repositorySourceId.value = sourceId;
  expandedSourceIds.value = initialExpandedSourceIds();
  preview.value = undefined;
}

function toggleSource(source: CloudSource): void {
  Object.assign(source, toggleSourceSync(source));
}

function isSourceExpanded(sourceId: string): boolean {
  return expandedSourceIds.value.has(sourceId);
}

function toggleSourceExpanded(sourceId: string): void {
  expandedSourceIds.value = toggleExpandedSource(expandedSourceIds.value, sourceId);
}

async function runConfirmed(): Promise<void> {
  const action = confirmAction.value; confirmAction.value = undefined;
  if (!action) return;
  busy.value = "confirmed";
  try { await action.run(); }
  catch (reason) { error.value = reason instanceof Error ? reason.message : String(reason); }
  finally { busy.value = ""; }
}

async function saveAndClose(): Promise<void> {
  busy.value = "save";
  try { await persist(); emit("close"); }
  catch (reason) { error.value = reason instanceof Error ? reason.message : String(reason); }
  finally { busy.value = ""; }
}

onMounted(() => { closeButton.value?.focus(); void loadSourceStatuses(); if (repositorySource.value) void loadPreview(); });
onBeforeUnmount(() => window.clearTimeout(toastTimer));
</script>

<template>
  <div class="dialog-backdrop" @pointerdown="backdrop.pointerDown" @pointerup="backdrop.pointerUp" @pointercancel="backdrop.pointerCancel">
    <section class="cloud-center" role="dialog" aria-modal="true" aria-labelledby="cloud-title">
      <header><div class="heading-icon"><CloudCog :size="21" /></div><div><p>同步服务</p><h2 id="cloud-title">云端设置</h2></div><button ref="closeButton" aria-label="关闭云端设置" title="关闭云端设置" @click="emit('close')"><X :size="18" /></button></header>
      <nav aria-label="云端设置页面"><button :class="{ active: tab === 'repository' }" @click="tab = 'repository'; repositorySource && loadPreview()">云端仓库</button><button :class="{ active: tab === 'sources' }" @click="tab = 'sources'">同步源</button></nav>
      <main>
        <section v-if="tab === 'repository'" class="repository-page">
          <div v-if="!repositorySource" class="empty"><CloudCog :size="30" /><h3>尚未配置云同步源</h3><p>添加 WebDAV 后即可查看和管理远端 Chronicle 仓库。</p><button @click="tab = 'sources'; addSource()"><Plus :size="16" />添加同步源</button></div>
          <template v-else>
            <div class="repository-heading">
              <div><p class="repository-eyebrow">共享快照历史</p><ThemedSelect :model-value="repositorySourceId" :options="sourceOptions" label="查看云端仓库来源" @update:model-value="selectRepositorySource" /><p class="repository-description">按存档标题汇总远端时间节点；仓库配置由 Chronicle 自动维护。</p></div>
              <button :disabled="Boolean(busy)" @click="loadPreview"><RefreshCw :size="15" />刷新</button>
            </div>
            <div class="repository-summary" aria-live="polite"><span>远端 {{ remoteArchives.length }} 个存档</span><span>共 {{ remoteSnapshotTotal }} 个时间节点</span><span>{{ preview?.libraryId ? '仓库已初始化' : '等待首次上传' }}</span></div>
            <div class="repository-controls">
              <label class="archive-search"><Search :size="16" /><input v-model="cloudSearch" type="search" placeholder="搜索存档标题" aria-label="搜索云端存档标题" /></label>
              <button class="danger" :disabled="!selected.length || Boolean(busy)" @click="confirmDelete(selected)"><Trash2 :size="15" />删除所选</button>
            </div>
            <div v-if="busy === 'preview' && !preview" class="loading">正在读取远端清单…</div>
            <div v-else-if="remoteArchives.length" class="remote-table-wrap">
              <table class="remote-table">
                <thead><tr><th scope="col" class="selection-column"><span class="sr-only">选择</span></th><th scope="col">存档</th><th scope="col">同步方案</th><th scope="col">云端时间线</th><th scope="col">最后更新</th><th scope="col" class="actions-column">操作</th></tr></thead>
                <tbody>
                  <tr v-for="item in visibleRemoteArchives" :key="item.id">
                    <td class="selection-column"><input v-model="selected" type="checkbox" :value="item.id" :aria-label="`选择 ${item.name}`" /></td>
                    <td class="archive-name"><b :title="item.name">{{ item.name }}</b></td>
                    <td><ThemedSelect :model-value="item.syncMode" :options="syncOptions" :disabled="Boolean(busy)" :label="`${item.name} 同步方案`" @update:model-value="changeSyncMode(item, $event)" /></td>
                    <td><span class="timeline-count">{{ item.snapshotCount }} 个节点</span><small>{{ formatBytes(item.sizeBytes) }}</small></td>
                    <td class="updated-at">{{ formatUpdatedAt(item.updatedAt) }}</td>
                    <td><div class="item-actions"><button :disabled="Boolean(busy)" :aria-label="`同步 ${item.name}`" :title="`同步 ${item.name}`" @click="runItemAction(item, 'sync')"><RefreshCw :size="14" />同步</button><button :disabled="Boolean(busy)" :aria-label="`覆盖下载 ${item.name}`" :title="`覆盖下载 ${item.name}`" @click="confirmOverwrite(item, 'download')"><Download :size="14" /></button><button :disabled="Boolean(busy)" :aria-label="`覆盖上传 ${item.name}`" :title="`覆盖上传 ${item.name}`" @click="confirmOverwrite(item, 'upload')"><Upload :size="14" /></button></div></td>
                  </tr>
                </tbody>
              </table>
              <div v-if="!visibleRemoteArchives.length" class="list-empty">没有匹配的云端存档。</div>
            </div>
            <div v-else class="list-empty remote-empty">远端暂无存档，可以从本地存档执行覆盖上传。</div>
            <div class="repository-toolbar"><span><b>{{ preview?.sourceName ?? repositorySource.name }}</b><small>{{ preview?.libraryId ? `仓库 ${preview.libraryId}` : '尚未创建远端仓库' }}</small></span><div><button :disabled="Boolean(busy)" @click="loadPreview"><RefreshCw :size="15" />刷新</button><button class="danger" :disabled="!selected.length || Boolean(busy)" @click="confirmDelete(selected)"><Trash2 :size="15" />删除所选</button></div></div>
            <div v-if="busy === 'preview' && !preview" class="loading">正在读取远端清单…</div>
            <div v-else class="remote-list">
              <article v-for="item in preview?.items ?? []" :key="item.id" :class="{ protected: item.protected }">
                <input v-if="!item.protected" v-model="selected" type="checkbox" :value="item.id" :aria-label="`选择 ${item.name}`" /><span v-else class="config-mark"><FileJson :size="16" /></span>
                <span class="remote-copy"><b>{{ item.name }}</b><small>{{ item.protected ? '受保护的仓库配置' : `${item.snapshotCount} 个时间节点 · ${formatBytes(item.sizeBytes)}` }}</small></span>
                <ThemedSelect v-if="!item.protected" :model-value="item.syncMode" :options="syncOptions" :disabled="Boolean(busy)" :label="`${item.name} 同步方案`" @update:model-value="changeSyncMode(item, $event)" />
                <div class="item-actions"><button v-if="!item.protected" :disabled="Boolean(busy)" @click="runItemAction(item, 'sync')"><RefreshCw :size="14" />同步</button><button v-if="!item.protected" :disabled="Boolean(busy)" @click="confirmOverwrite(item, 'download')"><Download :size="14" />覆盖下载</button><button v-if="!item.protected" :disabled="Boolean(busy)" @click="confirmOverwrite(item, 'upload')"><Upload :size="14" />覆盖上传</button><span v-else>不可删除</span></div>
              </article>
              <div v-if="preview && preview.items.length <= 2" class="list-empty">远端暂无存档，可从本地存档执行覆盖上传。</div>
            </div>
          </template>
        </section>

        <section v-else class="sources-page">
          <div class="source-toolbar"><button @click="addSource"><Plus :size="15" />添加 WebDAV 兼容源</button><button @click="addGitHubSource"><Plus :size="15" />添加 GitHub 兼容源</button><button @click="addOpenDalSource"><Plus :size="15" />添加 OpenDAL</button></div>
          <div v-if="!draft.sources.length" class="empty compact"><Server :size="28" /><h3>没有同步源</h3><p>Chronicle 支持保存多个云端配置，同时只启用其中一个。</p></div>
          <article v-for="source in draft.sources" v-else :key="source.id" class="source-card" :class="{ active: source.syncEnabled, collapsed: !isSourceExpanded(source.id) }">
            <div class="source-title"><button class="source-toggle" type="button" :aria-label="`${isSourceExpanded(source.id) ? '折叠' : '展开'} ${source.name}`" :title="`${isSourceExpanded(source.id) ? '折叠' : '展开'} ${source.name}`" :aria-expanded="isSourceExpanded(source.id)" @click="toggleSourceExpanded(source.id)"><Server :size="17" /><b>{{ source.name }}</b><small>{{ source.provider === 'legacy_github' ? 'GitHub 兼容源' : source.provider === 'opendal' ? 'OpenDAL · ' + source.scheme : 'WebDAV 兼容源' }}</small><small class="source-active-badge" :class="{ paused: !source.syncEnabled }">{{ source.syncEnabled ? '同步中' : '已暂停' }}</small><ChevronDown :size="16" :class="{ closed: !isSourceExpanded(source.id) }" /></button><button class="icon-sync" :class="{ paused: !source.syncEnabled }" :aria-label="`${source.syncEnabled ? '暂停同步' : '开始同步'} ${source.name}`" :title="source.syncEnabled ? '暂停同步' : '开始同步'" @click="toggleSource(source)"><Pause v-if="source.syncEnabled" :size="15" /><Play v-else :size="15" /></button><button class="icon-danger" :aria-label="`删除 ${source.name}`" :title="`删除 ${source.name}`" @click="removeSource(source)"><Trash2 :size="15" /></button></div>
            <div v-show="isSourceExpanded(source.id)" class="source-body"><div v-if="source.provider === 'legacy_github'" class="fields"><label><span>名称</span><input v-model.trim="source.name" type="text" /></label><label><span>仓库</span><input v-model.trim="source.repository" type="text" placeholder="owner/repository" /></label><label><span>分支</span><input v-model.trim="source.branch" type="text" placeholder="main" /></label><label class="wide github-token-field"><span>访问令牌</span><div><input v-model="passwords[source.id]" type="password" autocomplete="current-password" :placeholder="savedCredentials[source.id] ? '已保存，留空保留' : '粘贴 GitHub 生成的访问令牌'" /><button type="button" @click="openGitHubPatPage"><ExternalLink :size="14" />在 GitHub 生成令牌</button></div><small>登录后直接生成带 repo 权限的令牌；GitHub 只显示一次，请复制后粘贴到这里。</small></label><label><span>新仓库名称</span><input v-model.trim="newRepositoryNames[source.id]" type="text" :placeholder="githubRepositoryName(source.id)" /></label><button class="create-repository-button" :disabled="Boolean(busy)" @click="createGitHubRepository(source)"><Plus :size="15" />创建私有仓库</button><label class="wide"><span>Chronicle 目录</span><input v-model.trim="source.remotePath" type="text" placeholder="/Chronicle" /></label></div><OpenDalSourceFields v-else-if="source.provider === 'opendal'" :source="source" :secrets="secrets[source.id] ?? (secrets[source.id] = {})" :credential-saved="savedCredentials[source.id]" :disabled="Boolean(busy)" /><div v-else class="fields"><label><span>名称</span><input v-model.trim="source.name" type="text" /></label><label><span>服务器地址</span><input v-model.trim="source.endpoint" type="url" placeholder="https://dav.example.com/remote.php/dav/files/user" /></label><label><span>用户名</span><input v-model.trim="source.username" type="text" autocomplete="username" /></label><label><span>密码</span><input v-model="passwords[source.id]" type="password" autocomplete="current-password" :placeholder="savedCredentials[source.id] ? '已保存，留空保留' : '请输入密码'" /></label><label class="wide"><span>远端目录</span><input v-model.trim="source.remotePath" type="text" placeholder="/Chronicle" /></label></div><button class="test-button" :disabled="Boolean(busy) || (source.provider === 'legacy_github' ? !source.repository || !source.branch : source.provider === 'opendal' ? false : !source.endpoint || !source.username)" @click="testSource(source)">{{ busy === `test:${source.id}` ? '测试中…' : source.provider === 'legacy_github' ? '测试仓库访问' : source.provider === 'opendal' ? '测试读写、列举与清理' : '测试连接与读写' }}</button><small v-if="requiresTest(source)" class="source-test-hint">配置尚未测试或已更改，保存前请重新测试。</small></div>
          </article>
          <fieldset><legend>请求控制</legend><label><span>元数据并发</span><input v-model.number="draft.maxConcurrentMetadataReads" type="number" min="1" max="4" /></label><label><span>传输并发</span><input v-model.number="draft.maxConcurrentTransfers" type="number" min="1" max="4" /></label><label><span>请求间隔（毫秒）</span><input v-model.number="draft.requestDelayMs" type="number" min="0" max="5000" step="50" /></label><label><span>重试次数</span><input v-model.number="draft.retryLimit" type="number" min="1" max="10" /></label></fieldset>
        </section>
      </main>
      <footer><button class="cancel" :disabled="Boolean(busy)" @click="emit('close')">取消</button><button class="save" :disabled="Boolean(busy)" @click="saveAndClose">{{ busy === 'save' ? '保存中…' : '保存云端设置' }}</button></footer>
    </section>
    <div v-if="toast" class="cloud-toast" :class="toast.type" :role="toast.type === 'error' ? 'alert' : 'status'" aria-live="polite"><CheckCircle2 v-if="toast.type === 'success'" :size="16" /><CircleAlert v-else :size="16" /><span>{{ toast.message }}</span><button aria-label="关闭通知" title="关闭通知" @click="toast = undefined"><X :size="14" /></button></div>
    <ConfirmDialog v-if="confirmAction" :title="confirmAction.title" :message="confirmAction.message" confirm-label="确定" danger @cancel="confirmAction = undefined" @confirm="runConfirmed" />
  </div>
</template>

<style scoped>
.source-toolbar { flex-wrap: wrap; }
.source-test-hint { display: block; margin-top: 8px; color: var(--text-3); font-size: 11px; }
.dialog-backdrop{position:fixed;z-index:45;inset:0;display:grid;place-items:center;padding:28px;background:#18181b99;backdrop-filter:blur(3px)}.cloud-center{display:grid;grid-template-rows:72px 44px minmax(0,1fr) 64px;width:min(940px,calc(100vw - 56px));height:min(720px,calc(100vh - 56px));overflow:hidden;background:var(--surface);border:1px solid var(--border-2);border-radius:13px;box-shadow:0 24px 80px #0d24205c}.cloud-center>header{display:grid;grid-template-columns:42px 1fr 38px;align-items:center;gap:11px;padding:0 22px;border-bottom:1px solid var(--border)}.heading-icon{display:grid;place-items:center;width:38px;height:38px;color:var(--primary);background:var(--primary-soft);border-radius:9px}header p,header h2{margin:0}header p{color:var(--text-3);font-size:9px;font-weight:700;letter-spacing:.08em}header h2{margin-top:3px;font-size:18px}header button{display:grid;place-items:center;width:38px;height:38px;background:transparent;border-radius:7px}header button:hover{background:var(--hover)}nav{display:flex;gap:4px;padding:5px 22px 0;border-bottom:1px solid var(--border)}nav button{padding:0 14px;color:var(--text-3);background:transparent;border-bottom:2px solid transparent;font-size:11px;font-weight:650}nav button.active{color:var(--primary-dark);border-color:var(--primary)}main{overflow-y:auto;padding:20px 24px 28px}.message{display:flex;align-items:center;gap:7px;margin:0 0 12px;padding:9px 11px;border-radius:7px;font-size:10px}.message.error{color:#a52e28;background:#fff0ef}.message.success{color:var(--primary-dark);background:var(--primary-soft)}.empty{display:grid;place-items:center;min-height:390px;color:var(--text-3);text-align:center}.empty.compact{min-height:190px}.empty h3{margin:12px 0 0;color:var(--text);font-size:15px}.empty p{margin:7px 0 16px;font-size:10px}.empty button,.source-toolbar>button,.repository-toolbar button,.test-button{display:inline-flex;align-items:center;justify-content:center;gap:6px;min-height:34px;padding:0 10px;color:var(--primary-dark);background:var(--primary-soft);border-radius:7px;font-size:10px;font-weight:650}.repository-toolbar{display:flex;align-items:center;justify-content:space-between;gap:20px;margin-bottom:12px}.repository-toolbar>span{display:flex;flex-direction:column;gap:3px}.repository-toolbar b{font-size:13px}.repository-toolbar small{color:var(--text-3);font-size:9px}.repository-toolbar>div{display:flex;gap:7px}.repository-toolbar button.danger{color:#a52e28;background:#fff0ef}.remote-list{overflow:hidden;border:1px solid var(--border);border-radius:9px}.remote-list article{display:grid;grid-template-columns:28px minmax(150px,1fr) 118px auto;align-items:center;gap:10px;min-height:66px;padding:8px 11px}.remote-list article+article{border-top:1px solid var(--border)}.remote-list article.protected{background:#f7f9f8}.config-mark{display:grid;place-items:center;color:var(--text-3)}.remote-copy{display:flex;min-width:0;flex-direction:column;gap:4px}.remote-copy b{overflow:hidden;font-size:11px;text-overflow:ellipsis;white-space:nowrap}.remote-copy small{color:var(--text-3);font-size:9px}.remote-list select,.source-toolbar select,.fields input,fieldset input{height:34px;padding:0 9px;color:#263431;background:#f8faf9;border:1px solid var(--border-2);border-radius:6px;font-size:10px}.item-actions{display:flex;gap:5px}.item-actions button{display:inline-flex;align-items:center;gap:4px;min-height:31px;padding:0 7px;color:var(--text-2);background:#f2f6f4;border-radius:6px;font-size:9px}.item-actions span{color:var(--text-3);font-size:9px}.list-empty,.loading{display:grid;place-items:center;min-height:150px;color:var(--text-3);font-size:10px}.source-toolbar{display:flex;align-items:end;gap:8px;margin-bottom:14px}.source-toolbar label{display:flex;min-width:240px;flex-direction:column;gap:5px}.source-toolbar label span,.fields label span,fieldset label span{color:var(--text-2);font-size:9px;font-weight:650}.source-toolbar button:disabled{color:var(--text-3);background:#f1f3f2;cursor:default}.source-card{margin-bottom:12px;padding:14px;border:1px solid var(--border);border-radius:9px}.source-card.active{border-color:#9fcfc5;box-shadow:0 0 0 2px #0d8b7d14}.source-title{display:flex;align-items:center;justify-content:space-between;margin-bottom:12px}.source-title>span{display:flex;align-items:center;gap:7px}.source-title b{font-size:11px}.source-title small{padding:3px 6px;color:var(--primary-dark);background:var(--primary-soft);border-radius:10px;font-size:8px}.icon-danger{display:grid;place-items:center;width:32px;height:32px;color:#a52e28;background:transparent;border-radius:6px}.icon-danger:hover{background:#fff0ef}.fields{display:grid;grid-template-columns:1fr 1.5fr 1fr 1fr;gap:10px}.fields label{display:flex;flex-direction:column;gap:5px}.fields label.wide{grid-column:1/-1}.test-button{margin-top:12px}.sources-page fieldset{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-top:16px;padding:14px;border:1px solid var(--border);border-radius:9px}.sources-page legend{padding:0 6px;color:var(--text-2);font-size:10px;font-weight:700}.sources-page fieldset label{display:flex;flex-direction:column;gap:5px}.cloud-center>footer{display:flex;align-items:center;justify-content:flex-end;gap:8px;padding:0 22px;border-top:1px solid var(--border)}footer button{min-height:36px;padding:0 13px;border-radius:7px;font-size:10px;font-weight:650}.cancel{background:transparent}.cancel:hover{background:var(--hover)}.save{color:#fff;background:var(--primary)}button:disabled{cursor:default;opacity:.55}button:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--primary);outline-offset:2px}@media(max-width:900px){.fields{grid-template-columns:1fr 1fr}.sources-page fieldset{grid-template-columns:1fr 1fr}.remote-list article{grid-template-columns:28px minmax(120px,1fr) 110px}.item-actions{grid-column:2/-1}.cloud-center{width:calc(100vw - 28px);height:calc(100vh - 28px)}.dialog-backdrop{padding:14px}}
.repository-toolbar,.repository-toolbar + .loading,.remote-list{display:none}.repository-page{min-width:0}.repository-heading{display:flex;align-items:start;justify-content:space-between;gap:20px}.repository-eyebrow{margin:0;color:var(--text-3);font-size:10px;font-weight:700;letter-spacing:.07em}.repository-heading h3{margin:5px 0 0;color:var(--text);font-size:18px;line-height:1.2}.repository-description{margin:7px 0 0;color:var(--text-3);font-size:10px}.repository-heading>button{display:inline-flex;flex:0 0 auto;align-items:center;justify-content:center;gap:6px;min-height:34px;padding:0 10px;color:var(--primary-dark);background:var(--primary-soft);border-radius:7px;font-size:10px;font-weight:650}.repository-summary{display:flex;flex-wrap:wrap;gap:7px;margin:15px 0 12px}.repository-summary span{padding:4px 7px;color:var(--text-2);background:#f2f6f4;border-radius:12px;font-size:9px;font-variant-numeric:tabular-nums}.repository-summary span:last-child{color:var(--primary-dark);background:var(--primary-soft)}.repository-controls{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:12px}.archive-search{display:flex;align-items:center;gap:8px;min-width:0;width:min(360px,100%);height:36px;padding:0 10px;color:var(--text-3);background:#f8faf9;border:1px solid var(--border-2);border-radius:7px}.archive-search:focus-within{border-color:var(--primary);box-shadow:0 0 0 2px #0d8b7d18}.archive-search input{min-width:0;flex:1;height:100%;color:var(--text);background:transparent;border:0;font-size:10px}.archive-search input:focus{outline:0}.repository-controls .danger{display:inline-flex;align-items:center;justify-content:center;gap:6px;min-height:34px;padding:0 10px;color:#a52e28;background:#fff0ef;border-radius:7px;font-size:10px;font-weight:650}.remote-table-wrap{position:relative;overflow:visible;border:1px solid var(--border);border-radius:9px}.remote-table{width:100%;min-width:0;border-collapse:collapse;table-layout:fixed}.remote-table tbody tr:focus-within{position:relative;z-index:1}.remote-table th{height:34px;padding:0 9px;color:var(--text-3);border-bottom:1px solid var(--border);font-size:9px;font-weight:700;text-align:left}.remote-table td{height:58px;padding:7px 9px;border-bottom:1px solid var(--border);color:var(--text-2);font-size:10px;vertical-align:middle}.remote-table tbody tr:last-child td{border-bottom:0}.remote-table tbody tr:hover{background:#f8fbfa}.remote-table .selection-column{width:32px;padding-right:0;text-align:center}.remote-table .actions-column{width:168px}.remote-table .archive-name{width:25%;min-width:180px}.archive-name b{display:block;overflow:hidden;color:var(--text);font-size:11px;text-overflow:ellipsis;white-space:nowrap}.remote-table :deep(.themed-select){width:110px}.timeline-count{display:block;color:var(--text-2);font-variant-numeric:tabular-nums}.remote-table td small{display:block;margin-top:3px;color:var(--text-3);font-size:9px;font-variant-numeric:tabular-nums}.updated-at{color:var(--text-3)!important;font-variant-numeric:tabular-nums;white-space:nowrap}.remote-table .item-actions{display:flex;align-items:center;gap:5px}.remote-table .item-actions button{display:inline-flex;align-items:center;justify-content:center;gap:4px;min-width:30px;min-height:30px;padding:0 7px;color:var(--text-2);background:#f2f6f4;border-radius:6px;font-size:9px}.remote-table .item-actions button:first-child{color:var(--primary-dark);background:var(--primary-soft)}.remote-table .item-actions button:hover{background:var(--hover)}.remote-empty{border:1px dashed var(--border-2);border-radius:9px}.sr-only{position:absolute;width:1px;height:1px;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap}@media(max-width:900px){.repository-heading{align-items:start}.repository-controls{align-items:stretch;flex-direction:column}.archive-search{width:100%}.repository-controls .danger{align-self:flex-end}}
.github-token-field>div{display:flex;gap:8px}.github-token-field input{min-width:0;flex:1}.github-token-field button,.create-repository-button{display:inline-flex;align-items:center;justify-content:center;gap:6px;min-height:34px;padding:0 10px;color:var(--primary-dark);background:var(--primary-soft);border:1px solid transparent;border-radius:6px;font-size:10px;font-weight:650;white-space:nowrap}.github-token-field small{color:var(--text-3);font-size:9px;line-height:1.4}.create-repository-button{align-self:end}.github-token-field button:hover,.create-repository-button:hover{background:var(--hover);border-color:var(--border-2)}.github-token-field button:focus-visible{outline:2px solid var(--primary);outline-offset:2px}@media(max-width:900px){.github-token-field>div{align-items:stretch;flex-direction:column}}
.message.error, .repository-toolbar button.danger, .repository-controls .danger, .icon-danger { color: var(--danger); background: var(--danger-soft); }
.remote-list article.protected, .repository-summary span, .remote-table tbody tr:hover, .archive-search, .remote-list select, .source-toolbar select, .fields input, fieldset input, .item-actions button, .remote-table .item-actions button { background: var(--subtle); }
.remote-list select, .source-toolbar select, .fields input, fieldset input { color: var(--text); }
.source-card.active { border-color: color-mix(in srgb, var(--primary) 45%, var(--border)); box-shadow: 0 0 0 2px color-mix(in srgb, var(--primary) 12%, transparent); }
.icon-sync { display: grid; place-items: center; width: 32px; height: 32px; margin-left: 0; color: #2563eb; background: #eff6ff; border-radius: 6px; }.icon-sync:hover { background: #dbeafe; }.icon-sync.paused { color: #15803d; background: #ecfdf3; }.icon-sync.paused:hover { background: #dcfce7; }.icon-danger { margin-left: 0; }
.source-card.collapsed { padding-block: 10px; }.source-card.collapsed .source-title { margin-bottom: 0; }.source-title { justify-content: initial; gap: 8px; }.source-toggle { display: flex; min-width: 0; flex: 1; align-items: center; gap: 7px; padding: 4px; color: var(--text); background: transparent; border-radius: 6px; text-align: left; }.source-toggle:hover { background: var(--hover); }.source-toggle b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.source-toggle svg:last-child { margin-left: auto; flex: 0 0 auto; color: var(--text-3); transition: transform .14s ease; }.source-toggle svg.closed { transform: rotate(-90deg); }.source-title small.source-active-badge { color: var(--success); background: var(--success-soft); font-weight: 750; }.source-title small.source-active-badge.paused { color: var(--text-2); background: var(--subtle); }
.cloud-toast { position: fixed; z-index: 200; right: 28px; bottom: 28px; display: flex; align-items: flex-start; gap: 8px; max-width: min(390px, calc(100vw - 56px)); padding: 11px 12px; color: #fff; border: 1px solid transparent; border-radius: 9px; box-shadow: 0 12px 34px #0d242047; font-size: 11px; line-height: 1.45; }.cloud-toast span { flex: 1; }.cloud-toast button { display: grid; flex: 0 0 auto; place-items: center; width: 22px; height: 22px; margin: -3px -4px -3px 1px; color: inherit; background: transparent; border-radius: 5px; }.cloud-toast button:hover { background: #ffffff1f; }.cloud-toast.success { background: #17834a; border-color: #17834a; }.cloud-toast.error { background: #7f1d1d; border-color: #7f1d1d; }
.archive-search:focus-within { box-shadow: 0 0 0 2px color-mix(in srgb, var(--primary) 18%, transparent); }
.save { color: var(--on-primary); }
</style>
