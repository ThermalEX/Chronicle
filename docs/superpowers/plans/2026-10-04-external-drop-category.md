# 外部路径拖入与创建时选择分类 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将资源管理器中的文件／文件夹拖入窗口并预填一个新存档，同时在创建时选择分类，保留现有内部拖拽。

**Architecture:** Tauri 原生拖放事件提供绝对路径；独立只读后端命令描述路径，前端复用 `pendingSources` 与现有创建弹窗。先把内部 HTML5 拖拽换成指针事件，再启用原生拖放；分类选择直接使用现有 `CreateArchiveInput.categoryId`。

**Tech Stack:** Tauri 2 / Rust、Vue 3 / TypeScript、Vitest、Windows WebView2。

**Spec:** `docs/superpowers/specs/2026-10-03-external-drop-category-design.md`

## Global Constraints

- Windows 桌面版接收外部文件／文件夹；多个有效路径是一个存档的多个来源，绝不在 drop 时直接创建或备份。
- 不执行 EXE、不自动搜索游戏存档、不修改拖入的文件；无效路径明确提示，不能产生空存档。
- 保留分类／存档移入分类与回收站的语义和删除确认；菜单与键盘仍可替代指针拖动。
- 编辑现有存档的分类移动入口不变；浏览器开发模式不能声称支持绝对路径拖入。
- 在当前工作目录实施，不合并 fork 整支，也不清理现有未跟踪文件。

## Review Focus

1. 弹窗或教程期间的外部 drop：不覆盖当前草稿。由 Task 4 的阻塞状态测试覆盖。
2. 同一 Windows 路径大小写／斜杠不同、夹杂无效路径：只保留一份有效来源并显示被跳过项。由 Task 4 的归一化测试覆盖。
3. 列表滚动、点击按钮与轻微指针移动：不能误启动内部拖动。由 Task 3 的阈值与交互测试覆盖。
4. 把分类拖到自己的后代或拖到回收站：前者拒绝，后者保留删除确认。由 Task 3 的目标测试覆盖。
5. 创建弹窗打开后默认分类被删除：提交时报告分类不存在，不静默落到其他分类。由 Task 2 的提交测试覆盖。

---

### Task 1: 只读路径描述接口

**Files:** Create `apps/desktop/src-tauri/src/dropped_paths.rs`; modify `apps/desktop/src-tauri/src/lib.rs`; test in `dropped_paths.rs`.

**Interfaces:** Produce `describe_dropped_paths(paths: Vec<String>) -> Vec<DroppedPathDto>`, where each DTO has `path`, `name`, `kind: "file" | "folder"`, `exists`, and optional `error`. No file is opened for writing; actual source validation remains in `add_entry`.

- [ ] **Step 1: Write failing Rust tests** using `tempfile` for regular file, directory, missing path, invalid path containing NUL, Unicode names, and a mixed batch that retains valid items when one fails. Verify an unreadable directory manually on Windows if test permissions cannot reproduce it reliably.
- [ ] **Step 2: Run** `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml dropped_paths --lib`; expect missing module/API failure.
- [ ] **Step 3: Implement** classification with filesystem metadata only, then register the command in `lib.rs`. Reject non-file/non-directory paths and preserve per-item failure reasons without failing the entire batch.
- [ ] **Step 4: Run** targeted Rust test, `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check`, and `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --lib -- -D warnings`.
- [ ] **Step 5: Commit** only this task’s Rust files with `feat: describe dropped archive paths`.

### Task 2: 新建存档时选择分类

**Files:** Modify `apps/desktop/src/components/CreateArchiveDialog.vue`, `apps/desktop/src/App.vue`, `apps/desktop/src/locales/dialogs.ts`; create `apps/desktop/src/components/createArchiveCategory.test.ts`.

**Interfaces:** `CreateArchiveDialog` gains `categories: CategoryRecord[]`, `defaultCategoryId?: string`; `submit` emits the existing `CreateArchiveInput` with selected `categoryId`. `App.vue` uses `input.categoryId` in `archiveRepository.createArchive` rather than recomputing from current navigation. Existing edit dialog does not display this selector.

- [ ] **Step 1: Write failing Vitest cases**: current-category default, switching to another category/root, English labels, no selector in edit mode, and a category removed before submit causing a visible error rather than silent reassignment.
- [ ] **Step 2: Run** `npm run test -- src/components/createArchiveCategory.test.ts` from `apps/desktop`; expect missing prop/selector failures.
- [ ] **Step 3: Implement** an accessible, scrollable category picker with truncated long labels plus full title; pass current category and list from `App.vue`, and route the chosen ID unchanged to the existing repository create call. Reuse repository validation for vanished categories.
- [ ] **Step 4: Run** targeted test, `npm run typecheck`, and `npm run build` from `apps/desktop`.
- [ ] **Step 5: Commit** only this task’s files with `feat: select category when creating archive`.

### Task 3: 保留内部拖动的指针交互

**Files:** Create `apps/desktop/src/services/treePointerDrag.ts`, `apps/desktop/src/services/treePointerDrag.test.ts`; modify `apps/desktop/src/App.vue`, `apps/desktop/src/styles.css`.

**Interfaces:** Produce a small state machine `createTreePointerDrag()` exposing `start(item, point)`, `move(point, target, targetAllowed)`, `finish(): target | undefined`, `cancel()` and active item/target state. App callbacks continue using existing `canDropOnCategory`, `moveArchiveToCategory`, `moveCategory`, and deletion confirmation; pointer controller does not touch the repository.

- [ ] **Step 1: Write failing Vitest cases** for movement below/above a 5 px threshold, `cancel`/Esc, valid root/category/trash targets, a descendant target supplied as `targetAllowed=false`, and cancellation returning no target. Keep click and menu controls out of drag start.
- [ ] **Step 2: Run** `npm run test -- src/services/treePointerDrag.test.ts` from `apps/desktop`; expect missing API failure.
- [ ] **Step 3: Implement** the controller and replace HTML `draggable`/`dragstart`/`drop` handlers with pointer events, pointer capture, drag ghost and target hit testing. Keep the existing menu/keyboard paths and deletion confirmation. Add cleanup on unmount and when modal opens.
- [ ] **Step 4: Run** targeted test, `npm run typecheck`, and `npm run build`; in Windows app manually verify long-list scrolling, click-versus-drag, category nesting/root, trash confirmation and Escape.
- [ ] **Step 5: Commit** only this task’s files with `feat: preserve archive moves with pointer drag`.

### Task 4: 外部原生拖入接入创建流程

**Files:** Create `apps/desktop/src/services/droppedSources.ts`, `apps/desktop/src/services/droppedSources.test.ts`; modify `apps/desktop/src/App.vue`, `apps/desktop/src/styles.css`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src/locales/workspace.ts`, `README.md`.

**Interfaces:** Consume Task 1 DTO and Task 2 create flow. `normalizeDroppedSources(items: DroppedPathDto[], existing: ArchiveSource[])` returns accepted sources plus skipped paths/reasons, deduplicated case-insensitively on Windows. `canAcceptExternalDrop(state: { loading: boolean; dialogOpen: boolean; tutorialActive: boolean }): boolean` guards prefill. `App.vue` subscribes to `getCurrentWebview().onDragDropEvent` only in Tauri and opens the existing create dialog on an accepted drop.

- [ ] **Step 1: Write failing Vitest cases** for one file, one folder, mixed paths, case/slash duplicates, all-invalid and partially invalid batches, and a blocked dialog/tutorial state that leaves `pendingSources` untouched.
- [ ] **Step 2: Run** `npm run test -- src/services/droppedSources.test.ts` from `apps/desktop`; expect missing API failure.
- [ ] **Step 3: Implement** normalization, native event subscription/unsubscription, a non-interactive full-window drop overlay, and create-dialog prefill. Set `dragDropEnabled: true` only after Task 3 is green. Block external drop during dialogs/tutorial/loading; never auto-submit. Add Chinese/English text and a short README section.
- [ ] **Step 4: Run** targeted test, full `npm test`, `npm run typecheck`, `npm run build`, Rust dropped-path tests, and `git diff --check`. In Windows desktop, drop real files/folders from Explorer, including Unicode and mixed valid/invalid paths, then cancel and confirm separately; retest every internal drag destination.
- [ ] **Step 5: Commit** only this task’s files with `feat: create archives from dropped paths`.

## Final verification

Run `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib`, `npm test`, `npm run typecheck`, `npm run build`, `git diff --check`, then check `git status --short`. Windows desktop proof must show external drop does not break internal pointer movement, category selection or trash confirmation. No release or version bump is included.
