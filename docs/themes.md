# 本机主题包

本机 `themes` 目录只保留完整 ZIP；素材不提交到 Git，也不捆绑进软件安装包。以下两个示例主题可从 [v1.3.6 发行页](https://github.com/ThermalEX/Chronicle/releases/tag/v1.3.6) 单独下载，使用同页的 `SHA256SUMS.txt` 校验：

- [牧濑红莉栖](https://github.com/ThermalEX/Chronicle/releases/download/v1.3.6/Makise-Kurisu.zip)（`Makise-Kurisu.zip`）：红色配色、壁纸、图标和五种 Rayburst 音效。
- [粉漫少女](https://github.com/ThermalEX/Chronicle/releases/download/v1.3.6/Pink-Anime-Girl.zip)（`Pink-Anime-Girl.zip`）：配色与壁纸，不替换图标或音效。旧包内的「粉蓝插画」名称不参与命名，导入以 ZIP 文件名为准。

发行附件使用英文文件名和中文显示标签。若希望软件中显示中文主题名，先按下载时的英文文件名校验，再将 ZIP 重命名为 `牧濑红莉栖.zip` 或 `粉漫少女.zip` 后导入；不需要解压或改动 JSON。

红莉栖主题已包含五种声音，无需另外导入声音包。独立声音包格式见下文。

在「设置 → 个性化 → 定制主题」点击主题库的虚线框选择，或直接拖入主题 ZIP。这是一个完整主题包，已包含配置和素材，无需手动解压，也不需要分别导入 JSON 和声音文件。
导入弹窗勾选颜色、背景、图标和音效；未勾选的内容不载入，颜色使用默认配置，不继承当前主题。确认后先预览，统一「保存设置」才正式生效。主题卡片右上角可删除或单独导出任意主题，标题栏右上角新增主题；取消设置不保存，导出也不保存。

- 暗红强调色 `#B83E49`，深色，面板透明度 35%，磨砂 8 px，默认固定图片。
- `icon.png`：用户提供的红莉栖头像，背景去除并补全为透明 PNG，原图未改。
- `background.jpg`：1920×1080，来自 [STEINS;GATE RE:BOOT 官方站](https://steinsgate.jp/reboot/ja-jp/)，[原图](https://steinsgate.jp/reboot/ja-jp/common/img/intro3.jpg)。
- 五个 Rayburst WAV 为用户提供的设备连接、中断、失败、通知和默认响声；仅 Chronicle 事件使用，不修改 Windows 音效。

README 实机截图使用红莉栖主题，在独立演示资料库中将磨砂调为 3 px，方便看清背景；本机原始主题包仍保留 8 px 默认值。

图片和声音版权属于各自权利人，授权未核实，不声明 GPL 授权，不包含在 Git 或默认发行资源中；分享前请自行确认授权。本机图片、图标、音效保存时复制到当前实例资产区，之后移动原文件不影响使用，不同步到云端。便携版资产保存在自身 `Chronicle-data` 中。

图标用于关于页、当前窗口、运行中任务栏和托盘；不修改 EXE、安装包、快捷方式本身的固定图标，Windows 已固定快捷方式可能保留缓存。

## 图片播放

支持不自动切换、定时随机（1–86400 整数秒，可用秒／分钟输入）、启动随机（每次完整启动随机一次，托盘恢复或重新保存不重新抽取）。多图随机不连续重复；新图预加载后 700ms 渐隐，减少动态效果时直接切换。设置打开或后台时暂停正式轮播，实际软件预览使用独立草稿轮播。

## ZIP 格式与导出

点击任意主题卡片右上角的向外箭头，选择 ZIP 保存位置，默认文件名为主题名称.zip。无需切换当前主题，导出该卡片草稿的全部图片、图标、音效和播放配置，不包含原始绝对路径、设备身份、存档或云端凭据；不会保存设置。导入时直接使用 ZIP 文件名（去掉 .zip）作为主题名称；重命名压缩包即可改名。新导出的 theme.json 不包含 name，旧包的 name 字段会被忽略。

根目录 `theme.json` 使用 `formatVersion: 2`：

```json
{
  "formatVersion": 2,
  "colorTheme": "custom",
  "customAccent": "#B83E49",
  "colorMode": "dark",
  "transparency": 35,
  "blurPx": 8,
  "icon": "icon.png",
  "wallpapers": ["wallpapers/001.jpg"],
  "selectedWallpaperIndex": 0,
  "wallpaperPlayback": "fixed",
  "intervalSeconds": 60,
  "sounds": {"enabled": false, "volume": 60, "files": {}}
}
```

`wallpaperPlayback` 为 `fixed`、`intervalRandom`、`startupRandom`。每个主题最多 32 张图片，单张 15 MiB；配置 64 KiB，压缩包及实际解压总量 256 MiB。只允许安全相对路径，禁止链接、加密和重复路径。`sounds.files` 仅允许五种事件；PCM WAV 每个最大 5 MiB、30 秒。旧 v1 的单张 `wallpaper` 转为列表并保持固定播放。

声音包 ZIP 根目录使用 `sounds.json`，`formatVersion: 1`，`name` 为包名，`sounds` 包含 `enabled`、`volume`（0–100）和 `files`（事件键为 `connected`、`disconnected`、`connectionFailed`、`notification`、`default`）。也可导入主题 ZIP 的音效。只替换明确提供的槽位；完整主题缺省音效时保持关闭，不继承其他主题音效。

## 素材与本机保存

粉蓝插画原图由用户提供，未裁剪或从软件截图提取。Rayburst 五个声音也是用户提供的 WAV，授权均未核实。主题包作为独立发行附件供本机导入，不纳入 Git，不作为默认安装资源，也不声明素材采用本项目 GPL 授权。

已打包的 ZIP 不依赖原素材文件夹。导入后由软件复制到当前实例的本机资产区；删除仓库内的旧打包素材不会影响已导入主题。素材仍受原版权约束，导出不授予新许可。
