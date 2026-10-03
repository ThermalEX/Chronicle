# 本机自定义壁纸 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Windows 桌面版增加仅本机保存的图片壁纸、可调面板透明度与磨砂度，同时保留现有主题色和明暗模式。

**Architecture:** 独立的 Tauri 壁纸存储模块验证并复制图片到应用本机数据目录；前端壁纸服务持有当前与临时预览状态，CSS 只在图片模式改变主布局面板。设置弹窗保存两份设置，云端应用设置协议与资料库设置结构不新增壁纸字段。

**Tech Stack:** Rust/Tauri 2、Vue 3/TypeScript、Vitest、CSS 变量。

**Spec:** `docs/superpowers/specs/2026-10-03-local-wallpaper-design.md`

## Global Constraints

- 仅 Windows 桌面版完整支持；浏览器预览不得声称图片已持久保存。
- 仅 PNG、JPEG、WebP，按内容验证，最大 15 MiB；透明度 0–45%，默认 28%；模糊 0–24 px，默认 12 px。
- 壁纸偏好和图片只在 Tauri 本机应用数据目录；不得进入 `config/settings.json` 或云端 `app-settings.json`。
- 主题纯色模式原样保留；已选图片在切回纯色后仍可复用；未保存关闭必须还原预览。
- 在当前工作目录实施，不清理现有未跟踪文件，不合并 fork 整支。

## Review Focus

1. 原图片被移动：保存的副本仍显示。由 Task 1 的重载测试覆盖。
2. 图片数据伪装扩展名或超过 15 MiB：拒绝且旧图片不丢。由 Task 1 的格式和事务测试覆盖。
3. 云端设置下载或资料库根目录变化：本机壁纸不变。由 Task 1 的独立存储路径测试和 Task 2 的服务测试覆盖。
4. 保存应用设置成功、壁纸保存失败：弹窗不关闭，提示部分完成并可重试。由 Task 3 的弹窗测试覆盖。
5. 亮色图片、系统明暗切换与最小窗口：控件仍可读且有焦点轮廓。由 Task 3 的样式检查和桌面验收覆盖。

---

### Task 1: 本机壁纸存储与图片验证

**Files:** Create `apps/desktop/src-tauri/src/wallpaper.rs`; modify `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/Cargo.toml`; test in `wallpaper.rs`.

**Interfaces:** Produce Tauri commands `load_local_wallpaper() -> WallpaperView`, `preview_local_wallpaper(path: String) -> String` (data URL), `save_local_wallpaper(request: WallpaperSaveRequest) -> WallpaperView`. `WallpaperSaveRequest` has `mode: "color" | "image"`, `transparency: u8`, `blur_px: u8`, optional `source_path`, `remove_image`; `WallpaperView` adds optional `image_data_url` and `warning`. Helpers accept an explicit root `&Path` so tests use `tempfile` instead of real user data.

- [ ] **Step 1: Write failing Rust tests** in `wallpaper.rs` for defaults, saving/reloading a 1×1 PNG after deleting its original, JPEG/WebP acceptance, wrong-signature/oversize rejection, and corrupt preferences or missing image falling back to color. Assert that a rejected replacement leaves the previous image and preferences readable; assert the test root is outside a repository fixture.
- [ ] **Step 2: Run** `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml wallpaper --lib`; expect compilation/test failure because the module API is missing.
- [ ] **Step 3: Implement** the three commands and root-taking helpers. Use Tauri app-local-data path, a decoder restricted to PNG/JPEG/WebP, a 15 MiB size check before decode, a unique staged image filename, then write preferences and retire the prior file only after the new state is committed. Return base64 data URL rather than widening asset-protocol scope. Register commands in `lib.rs`.
- [ ] **Step 4: Run** the targeted Rust test until green; run `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check` and `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --lib -- -D warnings`.
- [ ] **Step 5: Commit** only this task’s Rust files with `feat: store local wallpaper safely`.

### Task 2: 壁纸状态、应用启动与主题感知面板

**Files:** Create `apps/desktop/src/services/wallpaper.ts`, `apps/desktop/src/services/wallpaper.test.ts`; modify `apps/desktop/src/App.vue`, `apps/desktop/src/styles.css`.

**Interfaces:** Consume Task 1 commands. Produce `WallpaperState { mode, transparency, blurPx, imageDataUrl?, warning? }`, `currentWallpaper`, `loadLocalWallpaper(): Promise<void>`, `previewLocalWallpaper(path: string): Promise<string>`, `saveLocalWallpaper(draft: WallpaperSaveRequest): Promise<void>`, `applyWallpaper(state: WallpaperState): void`. `applyWallpaper` sets root data mode and CSS variables without changing `colorTheme` or `colorMode`.

- [ ] **Step 1: Write failing Vitest cases** for default color mode, wallpaper CSS variables, switching back to color without losing the image, Tauri load/save roundtrip, and the absence of wallpaper keys in the existing `appSettings` payload. Include a test that cloud-style `saveAppSettings`/reload leaves `currentWallpaper` unchanged.
- [ ] **Step 2: Run** `npm run test -- src/services/wallpaper.test.ts` from `apps/desktop`; expect missing API failures.
- [ ] **Step 3: Implement** the service and load it after existing `initializeSettings()` in `App.vue`. Add wallpaper-only CSS selectors for the app shell, titlebar, sidebar, archive panel and detail panel; keep inputs, menus, cards and dialogs more opaque. Use existing light/dark theme tokens for the translucent tint and respect system-mode changes.
- [ ] **Step 4: Run** targeted Vitest, `npm run typecheck`, and `npm run build` from `apps/desktop`; verify color mode leaves the current layout rules unchanged.
- [ ] **Step 5: Commit** only this task’s files with `feat: render local wallpaper in app shell`.

### Task 3: 设置控件、草稿预览与保存确认

**Files:** Modify `apps/desktop/src/components/SettingsDialog.vue`, `apps/desktop/src/components/settingsDraft.test.ts`, `apps/desktop/src/locales/settings.ts`, `README.md`.

**Interfaces:** Consume Task 2 wallpaper service. Settings draft has `mode`, `sourcePath?`, `removeImage`, `transparency`, `blurPx`; it is separate from `AppSettings`. Existing `requestClose`, `discardClose`, and `save` remain the only close/save paths.

- [ ] **Step 1: Write failing component tests**: background type controls render before accent/theme controls; adjusting sliders previews but does not persist; discard restores saved wallpaper; image mode without a valid image blocks save; application settings success plus wallpaper failure keeps dialog open with a partial-save message; subsequent retry succeeds; English strings render.
- [ ] **Step 2: Run** `npm run test -- src/components/settingsDraft.test.ts` from `apps/desktop`; expect missing controls and draft behavior.
- [ ] **Step 3: Implement** image picker, radio/background selector, accessible sliders with visible values, miniature preview, and remove-image action. Extend `hasChanges`, save/discard and Escape flows; use existing partial-save pattern and keep errors next to the wallpaper controls. Add Chinese-to-English messages and a short README description.
- [ ] **Step 4: Run** targeted test, full `npm test`, `npm run typecheck`, `npm run build`, and `git diff --check`. In Windows desktop, inspect light/dark/system themes, 1024×720 window, opacity extremes, corrupted or missing image, keyboard slider control, restart persistence and cloud-settings download.
- [ ] **Step 5: Commit** only this task’s files with `feat: configure wallpaper from appearance settings`.

## Final verification

Run `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml wallpaper --lib`, `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib`, `npm test`, `npm run typecheck`, `npm run build`, and `git diff --check`; record any Windows desktop checks not automated. Do not claim the feature complete until the saved image survives restart and a cloud settings download does not change it.
