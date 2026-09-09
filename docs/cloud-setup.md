# Chronicle 云端同步配置指南

本指南对应 Chronicle **v1.1.0-beta** 与 Apache OpenDAL **0.59.1**。在程序中打开“云端设置 → 同步源”，选择合适的兼容源或 OpenDAL 服务模板。示例中的名称和密钥均为占位符。

## 通用规则

1. `Chronicle 目录` 是远端桶、容器或仓库内的前缀，推荐 `/Chronicle`；它不是本机磁盘路径，不能填 `C:\...`。
2. 先在服务商控制台创建 bucket、container 或 repository，再填写 Chronicle 表单。
3. 机密字段保存在 **Windows 凭据管理器**，不写入资料库 JSON。保存后留空代表保留旧值；删除字段才是清除旧值。不要把 Token、访问密钥或 GCS JSON 发到聊天、截图或提交到 Git。
4. 点击“测试读写、列举与清理”。程序只在 `.chronicle-probe-<UUID>/` 临时前缀下写入、读取、列举和删除测试对象；不通过则不能启用该源。
5. 修改 endpoint、桶、目录或任一密钥后必须重新测试；不要让多台设备同时向同一份资料库写入。

`endpoint` 必须是完整 HTTP(S) 地址，例如 `https://example.com`；不要附带用户名、密码、查询参数或 `#` 片段。

## 旧兼容源

### GitHub 兼容源

适合需要指定分支、创建私有仓库或 Git 批量提交的场景。

| 表单字段 | 怎么填 |
| --- | --- |
| 名称 | 自定义，例如 `GitHub 家用库` |
| 仓库 | `所有者/仓库名`，例如 `ThermalEX/chronicle-library` |
| 分支 | 例如 `main` |
| 访问令牌 | 仅授予目标仓库 Contents 读写权限的 fine-grained PAT；程序内建仓入口按提示使用 classic PAT 的 `repo` 范围 |
| Chronicle 目录 | `/Chronicle` |

### WebDAV 兼容源

| 表单字段 | 怎么填 |
| --- | --- |
| 服务器地址 | 服务商给出的 WebDAV 根地址；坚果云常见为 `https://dav.jianguoyun.com/dav/` |
| 用户名 | WebDAV 用户名或账号 |
| 密码 | 服务商生成的应用密码，不是网页登录密码 |
| 远端目录 | `/Chronicle` |

坚果云优先使用此兼容源，以保留覆盖上传回退行为。

## OpenDAL 模板

### S3 / 兼容 S3（AWS、Cloudflare R2、MinIO 等）

| 字段 | 怎么填 |
| --- | --- |
| `bucket` | 桶名称，例如 `chronicle-data` |
| `region` | AWS 例如 `us-east-1`；Cloudflare R2 通常填 `auto`；其他服务按控制台说明填写 |
| `endpoint` | 完整 S3 API Endpoint，例如 `https://s3.<region>.amazonaws.com` |
| `access_key_id` | S3 Access Key ID（机密） |
| `secret_access_key` | S3 Secret Access Key（机密） |

**Cloudflare R2**：选择此模板，不是 B2。Endpoint 填 `https://<Account ID>.r2.cloudflarestorage.com`，bucket 填 R2 bucket 名称，region 填 `auto`。在 R2 的 **Manage R2 API Tokens** 创建仅限目标 bucket、拥有对象读写权限的 token，将其 Access Key ID 和 Secret Access Key 分别填入机密字段。

**MinIO / 自建服务**：endpoint 通常是 `https://域名` 或 `http://局域网地址:端口`。仅在服务商文档明确要求时使用高级键值字段。

### Backblaze B2

选择 **Backblaze B2** 模板；它使用 B2 原生 API，**不要填写 S3 Endpoint**。

| 字段 | Backblaze 控制台中的名称 |
| --- | --- |
| `bucket` | Bucket Name，例如 `chronicle-data` |
| `bucket_id` | **Bucket ID**，不是账号 ID、Key ID 或 Endpoint |
| `application_key_id` | App Keys 中的 `keyID`（机密） |
| `application_key` | App Keys 中的 `applicationKey`（机密） |

在 **App Keys** 创建仅限目标 bucket、具文件读写、列举与删除能力的 key；在 bucket 详情页复制 Bucket ID。

### Azure Blob Storage

| 字段 | 怎么填 |
| --- | --- |
| `container` | Blob Container 名称 |
| `account_name` | Storage account 名称 |
| `endpoint` | `https://<account_name>.blob.core.windows.net`，或私有云 Endpoint |
| `account_key` | Storage account access key（机密） |

### Google Cloud Storage

| 字段 | 怎么填 |
| --- | --- |
| `bucket` | GCS bucket 名称 |
| `credential` | 服务账号 JSON 的**完整内容**（机密），不是 JSON 文件路径 |

创建服务账号，授予目标 bucket 对象读写、列举和删除权限；创建 JSON key 后，将整个 JSON 粘贴到 `credential`。不要填写 `GOOGLE_APPLICATION_CREDENTIALS` 或本地文件路径。

### 阿里云 OSS

| 字段 | 怎么填 |
| --- | --- |
| `bucket` | OSS bucket 名称 |
| `endpoint` | 例如 `https://oss-cn-hangzhou.aliyuncs.com` |
| `access_key_id` | RAM 用户 AccessKey ID（机密） |
| `access_key_secret` | RAM 用户 AccessKey Secret（机密） |

使用 RAM 子用户并把权限限制在目标 bucket。

### 腾讯云 COS

| 字段 | 怎么填 |
| --- | --- |
| `bucket` | 完整 bucket 名称，通常包含 AppID，例如 `chronicle-1250000000` |
| `endpoint` | 例如 `https://cos.ap-guangzhou.myqcloud.com` |
| `secret_id` | 子账号 API SecretId（机密） |
| `secret_key` | 子账号 API SecretKey（机密） |

### 华为云 OBS

| 字段 | 怎么填 |
| --- | --- |
| `bucket` | OBS bucket 名称 |
| `endpoint` | 例如 `https://obs.cn-north-4.myhuaweicloud.com` |
| `access_key_id` | IAM 用户 Access Key ID（机密） |
| `secret_access_key` | IAM 用户 Secret Access Key（机密） |

### 火山引擎 TOS

| 字段 | 怎么填 |
| --- | --- |
| `bucket` | TOS bucket 名称 |
| `endpoint` | 例如 `https://tos-cn-beijing.volces.com` |
| `region` | bucket 所在区域，例如 `cn-beijing` |
| `access_key_id` | Access Key ID（机密） |
| `secret_access_key` | Secret Access Key（机密） |

### OpenStack Swift

| 字段 | 怎么填 |
| --- | --- |
| `container` | Swift container 名称 |
| `endpoint` | Keystone 或 Swift API 提供的完整 Endpoint |
| `token` | 已获取的 Swift 访问 Token（机密） |

该模板只接受已经获取的 Token；Token 过期后重新获取并重新测试。

### GitHub（OpenDAL，默认分支）

| 字段 | 怎么填 |
| --- | --- |
| `owner` | GitHub 用户名或组织名 |
| `repo` | 已存在的仓库名 |
| `token` | 对该仓库有 Contents 读写权限的 PAT（机密） |

该模板仅使用仓库默认分支，不能选分支或建仓；需要这些功能请使用 GitHub 兼容源。

### WebDAV（OpenDAL）

| 字段 | 怎么填 |
| --- | --- |
| `endpoint` | 服务商 WebDAV 地址 |
| `username` | WebDAV 用户名 |
| `password` | 应用密码或 WebDAV 密码（机密） |

适用于标准 WebDAV 服务；坚果云仍建议使用旧 WebDAV 兼容源。

## 高级配置与状态说明

- 高级字段名只能使用小写英文、数字和下划线；未知字段默认按机密保存。
- Chronicle 自己管理 `root`。不要添加 `root`、`branch`、`credential_path`、`google_application_credentials`、`aws_profile`，以及任何以 `_path`、`_file` 结尾的字段。
- S3 临时凭据可添加机密字段 `session_token`。其他高级字段仅在服务商文档明确要求时使用。
- 测试的绿色圆点表示该同步源的当前配置已通过；改动任一字段、密钥或目录后失效，需要再次测试。
- 首页黄色“未检测”表示尚未执行健康检查；设置中开启“启动时检测云端”后，程序会逐个检测。红色状态会显示无法使用的同步源名称。

参考：[Apache OpenDAL 服务目录](https://opendal.apache.org/services/) · [OpenDAL 0.59.1 API](https://docs.rs/opendal/0.59.1/opendal/)
