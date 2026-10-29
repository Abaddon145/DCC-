# 课程批次种类隔离

2026-10-29。接在 `62b55e7` 之后，补完 schema v36 的笔记/课程批次隔离，并在本机用 Rust 1.89 复核迁入专项。合成临时库 only。Markdown 迁入和外部视频归档没有改写入路径，只是笔记批次显式写入 `batch_kind='notes'`，笔记批次列表不再带出课程批次。

## 行为

- `learning_import_batches.batch_kind` 只允许 `notes` 或 `courses`。旧库升级默认是 `notes`；升级失败时版本停在 35 且列不存在，已有笔记批次在成功升级后仍是 `notes`。
- 笔记侧的添加、暂存、提交、取消、撤回和条目列表遇到课程批次时失败，错误是「课程迁入批次不能使用笔记操作」。同一请求摘要不能同时被一个未结束的笔记批次和一个课程批次占用。
- 取消或撤回笔记批次不会改变旁边仍处于 pending 的课程批次。笔记请求摘要释放之后，课程批次才可以占用该摘要。
- 备份保留 `courses`，恢复后旧 epoch 不能继续创建课程批次。
- 课程来源登记的失败文案补全为「目标课程不可用」「目标系列不可用」「课程来源已关联或无法登记」「系列来源已关联或无法登记」。检查过新增文件，没有替换字符或截断到半个汉字的字符串。

## 本机验证

工具链 `rustc 1.89.0` / `cargo 1.89.0`，与 `src-tauri/Cargo.toml` 的 `rust-version` 一致。命令均为 `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib <filter> -- --test-threads=1`。

- `course_batch_`：3 通过。
- `course_origins_`：5 通过。
- `learning_import_course_preview`：8 通过。WAL 拷贝快照测试标了 `cfg(windows)`，这次没有跑。
- `learning_import`：87 通过。含 Markdown 批次、撤回、任务运行器和外部视频归档恢复。
- `upgrade_schema_guard`：14 通过。
- 同一会话里 schema 断言相关模块：`note_archive_tests` 7、`note_origins_tests` 8、`note_preview_tests` 13、`source_profiles_tests` 12、`db::tests` 14，均通过。摘录 v28 升级用例通过。
- `node scripts/qa/verify-dcc-learning-migration.mjs`：Rust 87、前端 `LearningImportPanel` 19，日志 `release-artifacts/yingliu-merge/migration-qa-5Z3x3S/`（已忽略目录）。脚本使用的 `cargo` 必须是 1.89；本机默认 1.83 会在依赖的 edition 2024 清单处失败。
- `cargo fmt --all -- --check` 和 `git diff --check` 通过。

## 非声明

- 没有课程迁入的添加、暂存、提交、取消或撤回实现，也没有课程预检的 Tauri 命令或界面。课程批次目前只是日志身份。
- 笔记批次列表故意不返回课程批次；这不是课程恢复界面。
- 没有真实映流 Vault、用户数据库、Cookie 或凭据。没有安装、NSIS、发布或合并到 main。
- 预检指纹和复核仍然不是提交许可。来源读取授权、媒体身份核验、决策持久化和真实数据迁移都没有做。
- 这次没有重跑 Windows 全量。Linux 上仍有与 schema 36 无关的既有失败：需要 `LOCALAPPDATA` 的本机访问批准、文案「下载进程暂仅支持 Windows」，以及缩略图/画面工具。那些失败不是把期望版本留在 35。
