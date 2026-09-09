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

当前源码版本为 **1.1.0**。此轮更新为源码适配，不包含新的安装包或 GitHub Release；截图展示已有工作区，新云端表单以当前程序为准。

## 功能

- 为单个文件或整个文件夹建立存档，按资料库分类管理。
- 创建不可变时间节点，比较变更、填写快照描述，并恢复到任意节点。
- 为每个时间节点保存备注；备注会写入资料库索引，云端同步后可在其他设备查看。
- 支持快照搜索、时间排序、打开本地压缩包、单节点同步及删除。
- 支持本地回收站、保留策略、错误记录与可复制的诊断信息。
- 保留 WebDAV 与 GitHub Repository 兼容同步源，新增 11 类 OpenDAL 服务模板及高级配置。
- 支持三方冲突处理、覆盖上传/下载；可保存多个同步源，每次使用一个活动同步源，不做多源并行镜像。
- OpenDAL 机密字段只存 Windows 凭据管理器；新配置必须完成读写、列举、删除及临时对象清理测试后才能启用。
- 支持可录制的全局快捷键、日间/夜间模式和青绿、靛蓝、紫罗兰、琥珀、玫红、灰色主题。

## 界面截图

| 主工作区 | 云端设置 |
| --- | --- |
| ![主工作区](docs/images/workspace.png) | ![云端设置](docs/images/cloud-settings.png) |

| 资料库管理 | 深色模式 |
| --- | --- |
| ![资料库管理](docs/images/library.png) | ![深色模式](docs/images/dark-mode.png) |

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

## 1.1.0：OpenDAL 云端同步

底层固定使用 Apache OpenDAL **0.59.1**。这不表示支持其网站列出的全部服务：当前仅编译下面的白名单，不包含个人网盘 OAuth 登录、数据库、缓存、只读或 Unix 专用后端。

| 类型 | scheme | 模板公开字段 | 模板机密字段 |
| --- | --- | --- | --- |
| S3 / S3 兼容服务 | `s3` | bucket、region、endpoint | access_key_id、secret_access_key |
| Backblaze B2 | `b2` | bucket、bucket_id | application_key_id、application_key |
| Azure Blob | `azblob` | container、endpoint、account_name | account_key |
| Google Cloud Storage | `gcs` | bucket | credential |
| 阿里云 OSS | `oss` | bucket、endpoint | access_key_id、access_key_secret |
| 腾讯云 COS | `cos` | bucket、endpoint | secret_id、secret_key |
| 华为云 OBS | `obs` | bucket、endpoint | access_key_id、secret_access_key |
| 火山引擎 TOS | `tos` | bucket、endpoint、region | access_key_id、secret_access_key |
| OpenStack Swift | `swift` | container、endpoint | token |
| GitHub | `github` | owner、repo | token |
| WebDAV | `webdav` | endpoint、username | password |

### 配置步骤

1. 打开云端中心 → 同步源，添加 OpenDAL 源并选择服务模板。
2. 填写目标容器/仓库、服务地址及机密字段。Chronicle 目录是容器内的独立前缀，例如 `/Chronicle`，不是本机磁盘目录。容器或仓库需事先存在。
3. 按服务需要添加高级键值字段。字段名遵循该版本 OpenDAL 的配置名称；未知字段默认按机密处理。`root` 统一由 Chronicle 目录填写，禁止额外 `branch`、`*_path`、`*_file` 读取本机凭据文件。
4. 点击测试。程序在唯一 `.chronicle-probe-<UUID>/` 下写入、读回、列举并清理测试对象，不初始化或改写 `library.json` 和 `catalog.json`。删除/清理失败会阻止启用，应先检查远端临时对象及权限。
5. 测试通过后保存并选择为活动源。修改配置或凭据后需要重新测试；已保存机密留空表示保留，删除字段表示不再使用该键。

例如 S3 兼容存储需要 `bucket`、服务商的 `endpoint` 和相应 `region`，密钥填到 `access_key_id`、`secret_access_key`；临时凭据可在高级字段添加机密 `session_token`。GCS 的 `credential` 按 OpenDAL 0.59.1 要求填写，不是本地凭据文件路径。公开 endpoint 不接受内嵌密码、查询令牌或片段。

### 兼容性、安全与限制

- 旧设置 v2 自动转换为 v3，`github` / `webdav` 只改为 `legacy_github` / `legacy_webdav` 标识；源 ID、凭据引用、目录、仓库及分支保持不变。远端资料库与 `.7z` 文件无需转换。
- 旧 GitHub 模式保留指定分支、创建仓库、批量 Git 提交；OpenDAL GitHub 仅使用默认分支，不替代前者。需要坚果云覆盖回退行为时继续使用旧 WebDAV 模式。
- `settings.json` 只存公开 config、secretKeys 和 credentialRef。Windows 凭据记录还绑定测试通过的配置，换设备需要重新输入凭据和测试；复制资料库不会复制系统凭据。
- 对象存储不要求原子目录重命名。上传先写快照，最后发布清单；失败不删除已存在的快照。覆盖上传可能保留不再被清单引用的旧对象；删除先移除清单引用，再清理目标前缀，清理失败会提示。
- 不提供跨设备事务或分布式锁；不要让多台设备同时覆盖同一资料库。连接测试只验证当时能力，不保证后续网络或授权持续有效。
- 请求有界重试、超时与并发限制；永久权限拒绝不重试。云服务费用、配额、单文件限制仍以服务商为准；当前文件传输会缓冲文件内容，大快照需预留内存。
- 自动化验证覆盖本地模拟/内存后端和协议回归，未使用真实账号逐一验收所有厂商。白名单服务仍必须以实际账号的能力测试为准。

参考：[Apache OpenDAL 服务目录](https://opendal.apache.org/services/) · [OpenDAL 0.59.1 API](https://docs.rs/opendal/0.59.1/opendal/)。
