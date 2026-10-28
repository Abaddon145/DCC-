# Task 12d1：持久迁入批次与条目恢复底稿

日期：2026-10-28；起点 `c017dd2`。schema 34 与内部 `import_ops` 仅服务合成/隔离库测试与后续任务中心接入；无 Tauri 命令、无 UI、无真实映流路径读取。

- 新增 `learning_import_batches` / `learning_import_items`：批次幂等（同库同 request_sha256）、epoch 校验、条目结构化来源键、runtime 暂存 `.dcc/runtime/imports/<batch>/<item>.md`（原子写入+摘要核对）。
- `commit_batch`：`Unmapped` 新建笔记并登记 `learning_origin_map`；`Unchanged` 不改正文；`SourceChanged`/`TargetChanged`/`BothChanged`/`TargetUnavailable` 标 conflict **绝不覆盖正文**；已提交清理暂存。`cancel_batch` 取消并删除暂存；`list_batches` 可见中断遗留 pending。
- `CURRENT_SCHEMA_VERSION` 升至 34；schema-v34 DDL 使用 IF NOT EXISTS 以兼容回退再迁移测试。
- **不能因此开放真实用户数据库/Vault 读取或迁入入口。** 调用方必须已提供核验字节；不接入任务中心 UI（12d2/12e）。
- 定向：`learning_import_ops` 5 项通过；`learning_import` 47 项通过；`learning_source_profiles`/`learning_note_origin`/`learning_source_vault`/`learning_note_archive`/`schema_guard` 相关回归通过。日志 `release-artifacts/yingliu-merge/task12d1-target-final.log`。完整 Rust/前端沿用 12c9 记录未重跑全量。未读取或迁移真实用户数据。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描 CheckedFiles=561 / PotentialSecrets=0。
