# Chronicle 多源同步与活动栏 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 支持多个可独立启停的云同步源，并优化存档页的创建快照和时间节点详情交互。

**Architecture:** 前端设置模型为每个 `CloudSource` 持久化 `syncEnabled`，并把旧的 `activeSourceId` 仅作为加载迁移依据。`App.vue` 使用一个来源列表并发执行存档页同步与自动上传；云端设置弹窗保留独立、非持久化的浏览来源选择。快照回调仅更新按钮内的进度状态，不再创建会挤压布局的页面级进度条。

**Tech Stack:** Vue 3、TypeScript、Vitest、Tauri 2、Rust。

**Spec:** `docs/superpowers/specs/2026-09-10-multi-source-sync-design.md`

## Global Constraints

- 新建同步源必须默认暂停，不能自动成为上传目标。
- 旧 `activeSourceId` 只能迁移启用一个旧来源，避免升级后意外向多个云端上传。
- 存档页同步与自动上传只发送给 `syncEnabled: true` 的来源，并发执行且错误互不阻塞。
- 暂停来源仍可在云端仓库页进行手动上传、下载、同步和管理。
- 使用 Lucide 图标与已有语义色；播放/暂停图标必须有可访问名称与文字状态。
- 页面级 `operation-progress` 必须删除；创建按钮尺寸不可因进度状态改变。

---

### Task 1: 多源设置模型与旧配置迁移

**Files:**
- Modify: `apps/desktop/src/services/settings.ts:36-215`
- Modify: `apps/desktop/src/services/settings.test.ts`
- Modify: `apps/desktop/src/services/opendal.test.ts`

**Interfaces:**
- Produces: `CloudSource.syncEnabled: boolean`。
- Produces: `enabledCloudSources(settings: Pick<CloudSettings, "enabled" | "sources">): CloudSource[]`，仅在 `settings.enabled` 时返回 `syncEnabled` 来源。

- [ ] **Step 1: 写入失败测试，定义旧单源配置的迁移结果**

```ts
expect(normalizedCloud({
  enabled: true,
  activeSourceId: "source-b",
  sources: [{ id: "source-a" }, { id: "source-b" }],
} as never).sources.map((source) => source.syncEnabled)).toEqual([false, true]);
```

- [ ] **Step 2: 运行迁移测试，确认它因缺少 `syncEnabled` 而失败**

Run: `npm test -- settings.test.ts opendal.test.ts`

Expected: FAIL，断言未得到 `[false, true]`。

- [ ] **Step 3: 在设置模型实现迁移和启用来源选择器**

```ts
export interface CloudSource { /* existing fields */ syncEnabled: boolean; }

export function enabledCloudSources(settings: Pick<CloudSettings, "enabled" | "sources">): CloudSource[] {
  return settings.enabled ? settings.sources.filter((source) => source.syncEnabled) : [];
}
```

`normalizedCloud` 必须在每个来源缺省 `syncEnabled` 时仅将旧 `activeSourceId` 对应项标记为 `true`；保存后的来源都持久化该字段。`activeSourceId` 从 `CloudSettings`、默认值、设置文档和云端指标输入中删除，指标改为根据启用来源数显示。

- [ ] **Step 4: 扩展测试覆盖新来源默认暂停、无有效旧来源全部暂停与启用来源选择器**

```ts
expect(enabledCloudSources({ enabled: true, sources })).toEqual([sources[1]]);
expect(normalizedCloud({ enabled: true, sources }).sources.every((source) => !source.syncEnabled)).toBe(true);
```

- [ ] **Step 5: 运行测试确认通过**

Run: `npm test -- settings.test.ts opendal.test.ts`

Expected: PASS。

- [ ] **Step 6: 提交设置模型任务**

```bash
git add apps/desktop/src/services/settings.ts apps/desktop/src/services/settings.test.ts apps/desktop/src/services/opendal.test.ts
git commit -m "feat: model independently enabled cloud sources"
```

### Task 2: 并发多源同步调度

**Files:**
- Create: `apps/desktop/src/services/multiSourceSync.ts`
- Create: `apps/desktop/src/services/multiSourceSync.test.ts`
- Modify: `apps/desktop/src/App.vue:141-148, 522-548`

**Interfaces:**
- Consumes: `enabledCloudSources`、`cloudRepository.sync`、`cloudRepository.upload`。
- Produces: `runAcrossEnabledSources(sources, action): Promise<SourceSyncOutcome[]>`，每项为 `{ source, status: "fulfilled" | "rejected", value?, reason? }`。

- [ ] **Step 1: 写入并发和失败隔离的失败测试**

```ts
const run = runAcrossEnabledSources(sources, async (source) => gates[source.id]);
await Promise.resolve();
expect(started).toEqual(["a", "b"]);
gates.a.resolve(); gates.b.reject(new Error("denied"));
expect((await run).map((outcome) => outcome.status)).toEqual(["fulfilled", "rejected"]);
```

- [ ] **Step 2: 运行测试，确认辅助函数尚不存在**

Run: `npm test -- multiSourceSync.test.ts`

Expected: FAIL，无法导入 `runAcrossEnabledSources`。

- [ ] **Step 3: 实现来源级 `Promise.allSettled` 包装**

```ts
export async function runAcrossEnabledSources<T>(sources: CloudSource[], action: (source: CloudSource) => Promise<T>): Promise<SourceSyncOutcome<T>[]> {
  return Promise.all(sources.map(async (source) => {
    try { return { source, status: "fulfilled" as const, value: await action(source) }; }
    catch (reason) { return { source, status: "rejected" as const, reason }; }
  }));
}
```

- [ ] **Step 4: 在 `App.vue` 使用辅助函数**

`queueAutomaticUpload` 和 `syncSelectedArchive` 均从 `enabledCloudSources(cloudSettings)` 收集来源。无启用来源时打开云端设置；执行时并发调用既有单来源 Tauri 命令；按来源构建成功、部分失败、全部失败通知并记录每个失败来源的诊断。删除全部 `cloudSettings.activeSourceId` 依赖。

- [ ] **Step 5: 运行多源测试与现有云服务测试**

Run: `npm test -- multiSourceSync.test.ts cloud.test.ts cloudHealthCheck.test.ts`

Expected: PASS。

- [ ] **Step 6: 提交同步调度任务**

```bash
git add apps/desktop/src/services/multiSourceSync.ts apps/desktop/src/services/multiSourceSync.test.ts apps/desktop/src/App.vue
git commit -m "feat: synchronize archives to enabled cloud sources"
```

### Task 3: 云端设置来源启停与浏览选择

**Files:**
- Modify: `apps/desktop/src/components/CloudCenterDialog.vue:1-345`
- Modify: `apps/desktop/src/components/iconButtonTooltips.test.ts`
- Create: `apps/desktop/src/services/cloudSourceControls.test.ts`

**Interfaces:**
- Consumes: `CloudSource.syncEnabled`、`saveCloudConfiguration`。
- Produces: 对话框本地 `repositorySourceId: Ref<string | null>`，仅用于 `preview` 和手动来源操作。

- [ ] **Step 1: 写入来源启停的失败测试**

```ts
expect(toggleSourceSync(source)).toMatchObject({ id: source.id, syncEnabled: true });
expect(newCloudSource("legacy_webdav", 1).syncEnabled).toBe(false);
```

- [ ] **Step 2: 运行测试，确认控制函数还不存在**

Run: `npm test -- cloudSourceControls.test.ts`

Expected: FAIL，无法导入来源控制函数。

- [ ] **Step 3: 实现纯来源控制函数并接入卡片**

创建 `apps/desktop/src/services/cloudSourceControls.ts`，实现 `toggleSourceSync` 和 `newCloudSource`。在 `CloudCenterDialog.vue` 使用 `Play`、`Pause`：删除按钮左边的状态按钮在暂停时显示绿色播放三角，在同步中显示蓝色暂停图标；两种状态都有标题、`aria-label` 与“同步中/已暂停”文字徽章。

- [ ] **Step 4: 移除同步源下拉菜单，分离仓库浏览选择**

将 `activeSource` 改为由本地 `repositorySourceId` 驱动的 `repositorySource`。首次打开时选择第一个配置来源；来源新增、删除或点击仓库选择器时仅改变预览，不改变任何 `syncEnabled`。所有仓库页手动操作使用 `repositorySource.id`，不检查其暂停状态。

- [ ] **Step 5: 验证来源操作和工具提示**

Run: `npm test -- cloudSourceControls.test.ts iconButtonTooltips.test.ts cloud.test.ts`

Expected: PASS。

- [ ] **Step 6: 提交云端设置界面任务**

```bash
git add apps/desktop/src/components/CloudCenterDialog.vue apps/desktop/src/components/iconButtonTooltips.test.ts apps/desktop/src/services/cloudSourceControls.ts apps/desktop/src/services/cloudSourceControls.test.ts
git commit -m "feat: control cloud sources independently"
```

### Task 4: 存档活动栏与按钮内嵌快照进度

**Files:**
- Create: `apps/desktop/src/services/snapshotButtonProgress.ts`
- Create: `apps/desktop/src/services/snapshotButtonProgress.test.ts`
- Modify: `apps/desktop/src/App.vue:80-84, 199-207, 499-519, 916-945`
- Modify: `apps/desktop/src/styles.css:212-282`

**Interfaces:**
- Consumes: `SnapshotProgress` 回调。
- Produces: `snapshotButtonProgress(progress?: SnapshotProgress): { state: "idle" | "indeterminate" | "determinate"; percent: number }`。

- [ ] **Step 1: 写入按钮进度映射的失败测试**

```ts
expect(snapshotButtonProgress({ current: 0, total: 0, currentPath: "scan" })).toEqual({ state: "indeterminate", percent: 0 });
expect(snapshotButtonProgress({ current: 3, total: 8, currentPath: "read" })).toEqual({ state: "determinate", percent: 38 });
```

- [ ] **Step 2: 运行测试，确认辅助函数不存在**

Run: `npm test -- snapshotButtonProgress.test.ts`

Expected: FAIL，无法导入 `snapshotButtonProgress`。

- [ ] **Step 3: 实现进度映射并改造唯一创建入口**

创建纯辅助函数。删除详情头部的“创建备份”按钮与页面级 `operation-progress`。仅保留时间节点区域“创建新快照”按钮；它的 `data-progress-state` 和 CSS 自定义属性 `--snapshot-progress` 由辅助函数驱动，执行中禁用且不改变宽高或文字布局。

- [ ] **Step 4: 将右侧详情改为活动栏**

保留选择时间节点后完整详情、备注和恢复操作。未选择时显示紧凑“活动栏”标题及提示，活动栏的内容容器不展开；选中节点后通过 `max-height`、`opacity` 与 `transform` 的短过渡展开。避免让时间线宽度、顶部高度或滚动位置跳动。

- [ ] **Step 5: 实现平滑按钮背景进度**

为创建按钮增加 `::before` 背景层：确定进度使用 `transform: scaleX(var(--snapshot-progress))` 和 180ms 线性过渡；扫描阶段使用同一背景层的 1.1s 循环平移动画；减少动态效果时保留静态 50% 填充。前景内容置于背景层上方。

- [ ] **Step 6: 运行进度测试和构建类型检查**

Run: `npm test -- snapshotButtonProgress.test.ts && npm run build`

Expected: PASS，且 Vue 类型检查无错误。

- [ ] **Step 7: 提交存档页交互任务**

```bash
git add apps/desktop/src/services/snapshotButtonProgress.ts apps/desktop/src/services/snapshotButtonProgress.test.ts apps/desktop/src/App.vue apps/desktop/src/styles.css
git commit -m "feat: streamline snapshot activity controls"
```

### Task 5: 整体验证与发布前检查

**Files:**
- Modify: `docs/superpowers/plans/2026-09-10-multi-source-sync-implementation.md`

- [ ] **Step 1: 运行完整前端套件**

Run: `npm test && npm run build`

Expected: 全部 Vitest 测试通过，Vue 类型检查和 Vite 生产构建通过。

- [ ] **Step 2: 运行 Rust 工作区与桌面端测试**

Run: `cargo test --workspace`

Expected: Rust 测试通过，云端单来源 Tauri 命令兼容新的 `syncEnabled` 配置字段。

- [ ] **Step 3: 手动验收桌面端**

Run: `npm run desktop:dev`

Expected: 新来源保持暂停；播放/暂停不影响其他来源；存档页同步只请求启用来源；仓库页可手动操作暂停来源；创建快照时页面不跳动；活动栏仅在选择节点后展开。

- [ ] **Step 4: 提交验证记录并推送 beta**

```bash
git add docs/superpowers/plans/2026-09-10-multi-source-sync-implementation.md
git commit -m "docs: record multi-source sync verification"
git push origin beta
```
