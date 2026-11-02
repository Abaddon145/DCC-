# Task 9.1 原字节归档检查点

日期：2026-10-24。起点 `7fdf5c9`，分支 `codex/yingliu-learning-integration`。本记录仅覆盖归档存储边界，不代表整项 Task 9 或真实映流数据迁入完成。

## 已实现

- schema v27 增加 `learning_note_archives` 与 `learning_note_archive_items`，记录源 profile/sourceKey/versionKey、摘要、库内固定路径、字节大小与片段范围。旧笔记 JSON、revision、更新时间不重写。
- 原字节保存到 `learning-data/archives/<UUID>.bin`。先核对输入摘要与 5 MiB 上限，再暂存、flush/fsync、独占发布、数据库事务登记；数据库失败清理本次独占文件。拒绝外层事务中发布文件，以免外层回滚留下虚假成功。
- 同源/版本/内容复用原 ID 前重新核对已存字节；源内容变化或来源 profile/版本不同保留独立记录。损坏副本不会被自动覆盖。
- 片段范围使用 UTF-8 字节偏移，不能切断字符；读取校验父原件和片段摘要。BOM、CRLF、未知 HTML/Wiki 内容原样保留，归档不执行 HTML。无效编码保留完整字节，片段解码明确失败。
- 读取仅接受已登记 UUID 与固定路径，复用学习资料路径校验；Windows 读句柄不共享写/删除。目录 junction 被拒绝；未注册文件、跨库 ID、路径登记篡改不能读取。
- 归档与登记随现有完整备份恢复。不引入新依赖、队列、源码编辑器或对源 Vault 的写入。

## RED → GREEN 与真实 I/O

`task9-archive-red.log` / `task9-archive-red-permitted.log`：存储桩返回未实现、缺少表及 v26 版本断言失败。首次沙箱阻止临时文件原子发布，获准重跑后暴露 junction 测试自身的 CMD 路径问题（带 `/archives` 被当开关）；测试改为两次 Path::join，产品未加入 shell。

`task9-archive-green.log`：7 项通过，含实际 600 KiB 读写、BOM/CRLF/Unicode 片段、内容与身份去重、损坏保护、SQL 拒绝与外层回滚、注册/路径边界、junction、完整备份恢复。随后增加精确 5 MiB 可接受边界及真实 v26 无表升级后旧 JSON/revision/更新时间不变测试，纳入最终完整回归。

## 最终验证

- 首次完整 Rust：421 通过、5 失败、2 既有 ignored。五处失败全部是最新 schema 预期仍写为 `26`；逐项读取实际错误后将最终版本断言更新为 `27`，保留资产身份、软删除、笔记、进度及备份内容断言。
- 最终 `cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`：**426 通过、0 失败、2 既有 ignored**，121.53 秒；main/doc tests 0，命令退出 0。日志 `release-artifacts/yingliu-merge/task9-archive-final-rust.log`。两项 ignored 仍为既有 20,000 笔记 benchmark 和真实网络图片验收，不计为本次通过。
- `npm test`：**365 项 / 72 文件通过**，44.53 秒；`npm run build`：退出 0，4.13 秒。日志 `task9-archive-full-ui.log` / `task9-archive-build.log`。生产保留原有大 chunk 提示。
- `cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check` / `git diff --check`：退出 0；凭据扫描 427 文件、0 命中，明确排除无关的 `Luca/`、`outputs/`、`tmp/`，未读这些目录。
- 本次没有真实用户数据迁入、Whisper 模型执行、付费 AI、安装覆盖、推送、合并或发布。归档测试均为隔离临时目录与合成字节。

## 未计为完成

本模块尚为后端内部边界，没有注册前端归档命令或自动迁入用户数据。typed 节点、转换预览/提交、包与 Markdown 导出及实际编辑交互继续在 Task 9.2–9.4 实施。真实源文件选择/只读快照/profile 映射属于 Task 12。异常断电可能留下未登记 UUID 文件，读取端不认领，后续恢复报告处理，不自动清理未知文件。真实 IME、物理 DPI/跨屏和用户数据验收仍另行记录。
