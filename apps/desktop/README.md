# Chronicle Desktop

该目录承载 Vue 3、TypeScript 和 Vite 桌面界面，后续由 Tauri 2 提供系统集成。

当前已经完成本地功能闭环：

- 通过 File System Access API 添加文件或文件夹。
- 将条目、文件句柄和快照内容持久化到 IndexedDB。
- 创建不可变时间节点，计算逐文件 SHA-256、总体内容哈希和变更统计。
- 恢复文件或目录；恢复前自动创建安全快照。
- 按分类与关键词查询，使用 `Ctrl+K` 聚焦搜索。

浏览器存储适配器位于 `src/services/archiveRepository.ts`。Tauri 接入后，界面继续使用相同领域模型，文件内容改由 Rust 本地仓库保存。

```powershell
npm install
npm run dev
npm test
npm run build
```
