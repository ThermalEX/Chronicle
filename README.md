<p align="center">
  <img src="docs/images/icon.png" width="144" alt="Chronicle 图标" />
</p>

<h1 align="center">Chronicle</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Vue-3-42B883?logo=vuedotjs&logoColor=white" alt="Vue 3" />
  <img src="https://img.shields.io/badge/Rust-1.97%2B-000000?logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/Snapshots-7z-5B8C85" alt="7z 快照" />
  <img src="https://img.shields.io/badge/Sync-OpenDAL%20%2B%20WebDAV%20%2B%20GitHub-64748B" alt="OpenDAL、WebDAV 和 GitHub 同步" />
</p>

Chronicle 是一个面向游戏存档、软件配置和任意文件夹的桌面时间线备份工具。它以本地资料库为基础，把每次备份保存为可恢复的 `.7z` 快照，并可同步到 WebDAV、GitHub 或 OpenDAL 对象存储。

当前正式版为 **1.3.2**；最新测试版为 **1.3.4-beta**，新增 Galgame 存档扫描。支持 Steam 存档识别、简体中文与英语界面、自动备份、自动上传、托盘后台运行与多源云端同步，可在应用内选择测试版渠道检查更新。

## 功能

- 扫描 Steam 多个游戏库的存档来源，按账号昵称分组，勾选后批量添加；扫描结果本地缓存，可选择手动刷新。
- Galgame 存档扫描（Beta）：选择合集文件夹，递归识别嵌套目录中的游戏，检查游戏内及常见 Windows 用户目录中的已有存档，核对后批量添加。
- 支持简体中文与英语，欢迎页和设置中均可切换，立即生效并保存选择。
- 首次使用提供语言选择与动态问候、主题预览及分步教程，涵盖创建存档、自动化、快照恢复、星标、同步和游戏存档识别；可跳过，也可在关于页重新开始。
- 为单个文件或整个文件夹建立存档，按资料库分类管理。
- 创建不可变时间节点，比较变更、填写快照描述，并恢复到任意节点；恢复前会先保存当前状态为安全快照。
- 按存档开启自动备份，监听文件或文件夹变化并创建快照；支持调整文件合并时间。
- 按存档开启自动上传，新快照会上传至全部未暂停的同步源；上传失败时保留本地快照并提供诊断信息。
- 关闭主窗口可最小化到托盘，继续后台监听；可从托盘显示或退出应用。
- 为每个时间节点保存备注；备注会写入资料库索引，云端同步后可在其他设备查看。
- 支持快照搜索、时间排序、打开本地压缩包、单节点同步及删除。
- 支持本地回收站、保留策略、错误记录与可复制的诊断信息。
- 保留 WebDAV 与 GitHub Repository 兼容同步源，新增 11 类 OpenDAL 服务模板及高级配置。
- 支持三方冲突处理、覆盖上传/下载；可保存并同时启用多个同步源，暂停某个来源不会影响其他来源。
- OpenDAL 机密字段只存 Windows 凭据管理器；新配置必须完成读写、列举、删除及临时对象清理测试后才能启用。
- 支持可录制的全局快捷键、日间/夜间模式和青绿、靛蓝、紫罗兰、琥珀、玫红、灰色主题。
- 关于页支持正式版/测试版更新频道、启动时检查与手动检查更新，可查看更新说明、打开下载链接或下载校验后启动 Windows 安装包。

## 备份保护与注册表

- 创建或编辑存档时可设置排除规则，例如 `*.tmp`、`cache/`。排除项不会进入新快照，也不会触发自动备份；恢复时保留当前规则和快照规则所保护的本机文件。
- 时间节点可加星锁定：锁定节点不占普通保留数量，不会被自动清理；手动删除前需要先解锁。锁定状态和备注可随云端同步。
- Windows 支持添加注册表子键来源，可与文件、文件夹放在同一存档。支持 `HKCU`、`HKLM` 的 64 位注册表视图；恢复可选合并或覆盖，覆盖需要再次确认，两者都会先创建安全快照。不自动提权，也暂不监听注册表变化。

## 界面截图

### 主工作区

![主工作区](docs/images/workspace.png)

### 云端设置

![云端设置](docs/images/cloud-settings.png)

### 资料库管理

![资料库管理](docs/images/library.png)

### 深色模式

![深色模式](docs/images/dark-mode.png)

## 技术栈

- Rust + Tauri 2：本地文件访问、压缩、同步与 Windows 系统集成。
- Vue 3 + TypeScript + Vite：桌面应用界面。
- JSON 索引 + `.7z` 快照：资料库元数据和版本文件。

## 本地开发

```bash
cd apps/desktop
npm install
npm run desktop:dev
```

常用验证命令：

```bash
cd apps/desktop
npm test
npm run typecheck
npm run build
cd src-tauri
cargo test
cargo clippy -- -D warnings
```

## Windows 发行版与资料库位置

- **安装版**：NSIS 安装向导支持选择安装位置；资料库始终保存到 `%LOCALAPPDATA%\com.thermalex.chronicle\Chronicle`，避免在 `Program Files` 等安装目录写入用户数据。
- **便携版**：解压后直接运行 `Chronicle.exe`。首次启动会在 EXE 同级创建 `Chronicle-data/` 作为新资料库；便携版不会读取、迁移或回退到 App Local Data。请保留 `Chronicle.exe`、`portable.marker` 与 `Chronicle-data/` 的同级关系。

## GitHub 同步权限

GitHub Repository 同步使用 Personal Access Token。已有仓库可使用限定目标仓库、具有 Contents 读写权限的 fine-grained token；程序中的令牌生成入口使用 classic token 的 `repo` 范围，供兼容同步和建仓使用。组织策略仍可能要求额外授权。令牌会写入 Windows 凭据管理器，设置文件只保存引用。

标准 GitHub 仓库模式会在 Chronicle 目录中维护：

```text
library.json
catalog.json
archives/<存档目录>/<时间>.7z
```

单个 `.7z` 快照超过 100 MB 时，GitHub 标准仓库 API 会拒绝上传；Chronicle 会在发起上传前提示该限制。

## 云端同步配置

云端同步基于 Apache OpenDAL，并保留 GitHub 与 WebDAV 兼容源。可配置多个同步源，按需启用或暂停。完整的逐服务商填写说明、字段对照、Cloudflare R2 与 Backblaze B2 示例，以及测试与安全注意事项见：[云端同步配置指南](docs/cloud-setup.md)。

## Steam 存档扫描

点击顶部工具栏的游戏手柄图标，打开「游戏存档识别」弹窗，选择带 Steam 图标的分页，自动识别 Windows 上的 Steam 安装目录和多个游戏库。也可以手动选择包含 `steamapps` 的 Steam 目录。扫描结果和时间保存在本机，再次打开直接显示上次结果，需要时点击「刷新扫描」。

扫描使用 [Ludusavi Manifest](https://github.com/mtkennerly/ludusavi-manifest)（数据主要来自 [PCGamingWiki](https://www.pcgamingwiki.com/)），匹配文件、文件夹、注册表及 Steam userdata 云存档目录。首次扫描需要联网下载路径库；后续使用本机缓存，点击「更新路径库并扫描」可检查更新。数据库不保证覆盖所有游戏。

核对并勾选实际存在的来源后，可批量添加为本地存档，选择是否立即创建第一份备份。已管理的来源会跳过；自动备份和自动上传默认关闭，可在存档编辑页开启。未找到存档时，可先运行游戏并保存一次，或手动添加来源。

扫描会读取本机 Steam 的昵称，将能够确认账号的来源按用户分组，添加后以“游戏名 · 昵称”命名。昵称不可用时显示账号名或用户 ID；无法判断所属账号的来源标为“本机存档”。昵称随扫描结果保存，刷新扫描时更新；旧缓存中明确位于 `userdata/<账号 ID>` 下的来源会在打开时补齐账号信息，无需重新扫描。

## Galgame 存档扫描（Beta）

在「游戏存档识别」窗口选择带书本图标的「Galgame」分页，选择一个合集文件夹后开始扫描。支持多层嵌套目录，识别到游戏目录后检查存档；扫描进度可见，也可随时取消。文件夹选择和完整扫描结果保存在本机，再次打开无需重新扫描。

离线规则覆盖 Kirikiri、Ren’Py、RPG Maker MV/MZ/XP/VX/VX Ace、Wolf RPG，以及可执行文件旁的常见存档目录。除游戏目录外，还检查 AppData、文档和 Saved Games 中按游戏名匹配的已有位置。结果注明引擎规则、通用目录或名称匹配，请核对后勾选；未生成存档的游戏只提示，根目录中的 RPG Maker 存档按文件添加。

本功能仅支持 Windows，不接入在线数据库，不执行游戏程序或脚本。链接目录、目录联接和云端占位文件会跳过；自定义引擎、加密配置和特殊存档位置可能需要手动添加。测试版仅提供本地安装包，尚未发布到 GitHub Release。

## 语言、外观与新手教程

首次使用先选择「简体中文」或「English」，欢迎页轮换显示 Hello、你好、Hola 等问候语，并尊重系统的减少动态效果设置。语言选择立即生效并保存，也可随时从「设置 → 软件 → 界面语言」更改；用户自己的存档名、分类名、路径和第三方原始错误保持原文。

教程支持主题与浅色／深色实时预览，随后引导配置云端或仅本地使用、创建存档、备份、恢复、星标与同步。最后一步可通过游戏手柄图标打开游戏存档识别窗口，选择 Steam 或 Galgame 分页；打开不会自动扫描。日间／夜间切换按钮位于整个页面右下角。

## 更新日志

[查看完整更新日志](docs/changelog.md)
