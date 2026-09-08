# Chronicle v1.0.0 发布设计

## 目标

发布 Windows x64 的 v1.0.0，提供可选安装目录的 NSIS 安装版，以及资料库位于软件目录的便携版。README 展示四张实际界面截图；关于页显示应用图标、版本和作者。

## 发布物

| 文件 | 用途 | 资料库位置 |
| --- | --- | --- |
| `Chronicle_1.0.0_x64-setup.exe` | NSIS 安装向导；允许选择当前用户或全局安装，并保留目录选择页。 | `%LOCALAPPDATA%\com.thermalex.chronicle\Chronicle` |
| `Chronicle-1.0.0-windows-x64-portable.zip` | 解压即用的便携版。 | `Chronicle-data/`，与 `Chronicle.exe` 同级。 |
| `SHA256SUMS.txt` | 两个发布物的 SHA-256 校验值。 | 不适用 |

安装版不将用户资料库写入安装目录：用户可选择的目录可能位于 `Program Files`，该位置通常没有普通用户的写权限。NSIS 使用 Tauri 的 `installMode: "both"`，以保留当前用户或全局安装选择与安装路径选择。

## 便携模式

- 便携包根目录包含空的 `portable.marker`。
- 启动时，后端先检查 EXE 同级的该标记；存在时以 `Chronicle-data/` 作为 `LocalRepository` 根目录，否则维持现有 App Local Data 根目录。
- 便携模式不读取、不迁移、也不回退到 App Local Data。若 EXE 同级没有 `Chronicle-data/`，直接在该目录新建空资料库。
- 安装版维持现有资料库位置，不做迁移。

## 版本与打包

- 将工作区 Cargo 版本、桌面 Cargo 版本、`package.json`、`package-lock.json` 和 `tauri.conf.json` 统一为 `1.0.0`。
- 在 Tauri 配置中启用 `bundle.active`，目标仅为 `nsis`；保留已配置的 Windows 图标。
- 新增 `scripts/package-portable.ps1`：复制 release EXE、写入 `portable.marker` 和便携版说明，生成确定命名的 ZIP。
- 以 `npm run desktop:build` 生成 NSIS 安装包，再以脚本生成便携 ZIP；不引入自动更新器和代码签名。发布说明明确标注当前为未签名 Windows 构建。

## 关于页

- 新增前端可访问的应用图标资源，并在“设置 → 关于”的品牌卡片中显示该图标，替换通用设置图标。
- 品牌卡片显示 `Chronicle`、版本 `1.0.0` 和作者 `ThermalEX`。
- 版本号由前端构建配置注入，确保与发布版本一致，不在组件中单独维护版本常量。

## README

- 将四个截图占位格替换为 `docs/images/workspace.png`、`cloud-settings.png`、`library.png`、`dark-mode.png` 的 Markdown 图片。
- 保留已存在的居中图标、标题和技术徽章。
- 开发章节增加安装版与便携版的资料库位置说明。

## GitHub Release

1. 运行前端测试、TypeScript 检查、前端构建、Rust 测试、Clippy、`git diff --check`。
2. 对安装包和 ZIP 计算 SHA-256，写入 `SHA256SUMS.txt`。
3. 创建并推送 `v1.0.0` annotated tag。
4. 使用 GitHub CLI 创建正式 GitHub Release，上传安装包、便携 ZIP 和校验文件；发布说明列出主要功能、两种资料库行为和未签名提示。

## 验收标准

- NSIS 安装器允许用户选择安装范围与安装目录；安装后的启动和资料库读写正常。
- 便携 ZIP 解压后，首次启动在 EXE 同级新建或打开 `Chronicle-data/`，且不会读取或创建 App Local Data 资料库；无 `portable.marker` 的构建继续使用 App Local Data。
- 关于页显示实际应用图标、构建注入的 `1.0.0` 版本号和作者 `ThermalEX`。
- README 在 GitHub 上显示四张截图，且所有相对图片链接可用。
- Release 中存在三份发布资产、tag 为 `v1.0.0`，所有发布物的 SHA-256 与校验文件一致。
