# 发布注意事项

## Beta 下载包命名

- Beta 安装包和便携包的文件名必须包含 `beta`，不能仅在 Release 标题或正文中标注。
- 安装包：`Chronicle_{版本}-beta_x64-setup.exe`。
- 便携包：`Chronicle-{版本}-beta-windows-x64-portable.zip`。
- 例如：`Chronicle_1.3.7-beta_x64-setup.exe`、`Chronicle-1.3.7-beta-windows-x64-portable.zip`。
- 多轮测试使用 `-beta.1`、`-beta.2` 等后缀；版本本身已带 Beta 后缀时直接使用完整版本，不重复添加 `beta`。
- `SHA256SUMS.txt` 保持原名，清单中的文件名必须与最终上传的附件名完全一致。

## 发布或改名时核对

- [ ] 在上传前检查最终文件名；不要仅凭构建成功或预发布标记判断命名正确。
- [ ] GitHub Beta Release 启用预发布标记，标题标注 Beta；新 Beta 标签使用 `v{版本}-beta`（或带测试序号），不设为最新正式版。
- [ ] 校验清单、Release 下载说明和相关文档链接同步使用最终文件名。
- [ ] 上传后核对远端附件名与 SHA-256，并确认校验清单可下载。
- [ ] 已发布正式版后来改为 Beta 时，同样补齐下载包的 Beta 后缀。仅改名时保留原标签和二进制内容，更新清单与说明，不重打包；GitHub 自动生成的源码归档随原标签，不属于手动上传的下载包。

发布正文统一维护在 [更新日志](changelog.md)，不另建逐版本正文副本。
