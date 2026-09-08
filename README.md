# Chronicle

Chronicle 是一个面向游戏存档、软件配置和任意文件夹的桌面时间线备份工具。它以本地资料库为基础，把每次备份保存为可恢复的 `.7z` 快照，并可同步到 WebDAV 或 GitHub 仓库。

## 功能

- 为单个文件或整个文件夹建立存档，按资料库分类管理。
- 创建不可变时间节点，比较变更、填写快照描述，并恢复到任意节点。
- 为每个时间节点保存备注；备注会写入资料库索引，云端同步后可在其他设备查看。
- 支持快照搜索、时间排序、打开本地压缩包、单节点同步及删除。
- 支持本地回收站、保留策略、错误记录与可复制的诊断信息。
- 支持 WebDAV 与 GitHub Repository 两类同步源；GitHub 令牌仅保存为 Windows 凭据引用。
- 支持三方冲突处理、覆盖上传/下载，以及多个已启用同步源的并行同步。
- 支持可录制的全局快捷键、日间/夜间模式和青绿、靛蓝、紫罗兰、琥珀、玫红、灰色主题。

## 界面截图

| 主工作区 | 云端设置 |
| --- | --- |
| 待放置：`docs/images/workspace.png` | 待放置：`docs/images/cloud-settings.png` |

| 资料库管理 | 深色模式 |
| --- | --- |
| 待放置：`docs/images/library.png` | 待放置：`docs/images/dark-mode.png` |

将对应 PNG 放入 `docs/images/` 后，把上表中的占位文本替换为 Markdown 图片即可。

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

## GitHub 同步权限

GitHub Repository 同步使用 Fine-grained Personal Access Token。令牌需要能访问目标仓库，并拥有 Contents 的读写权限；若要让 Chronicle 创建私有仓库，还需要账户级 Administration 的读写权限。令牌会写入 Windows 凭据管理器，设置文件只保存引用。

标准 GitHub 仓库模式会在 Chronicle 目录中维护：

```text
library.json
catalog.json
archives/<存档目录>/<时间>.7z
```

单个 `.7z` 快照超过 100 MB 时，GitHub 标准仓库 API 会拒绝上传；Chronicle 会在发起上传前提示该限制。
