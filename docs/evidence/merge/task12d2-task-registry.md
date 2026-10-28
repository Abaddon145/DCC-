# Task 12d2：迁入批次接入 TaskRegistry

日期：2026-10-28；起点 `6703b15`。将 schema34 `import_ops` 接到既有任务中心生命周期（与 CourseExcerpt 同型 Prepare/Commit/Cancel）；无 Tauri 产品命令、无 12e UI、无真实映流路径读取。

- 新增 `learning/import_runner.rs`：`ImportTaskRequest::{Prepare,Commit,Cancel}`；Prepare 校验 profile/epoch/hash/title/路径，并核对调用方已提供的 markdown 字节 SHA-256（≤5MiB），不打开 Vault。
- `TaskRequest::LearningImport`：校验/标签、活跃请求去重、`execute` 走 `import_runner::run`；Commit 成功且晚于停止请求时保留 `completed`（“迁入已提交…”），与摘录提交语义一致。
- `import_ops`：`add_item`/`stage_item_bytes` 对同键同内容与已暂存同字节幂等，便于 Prepare 重试而不留半份。
- **不能因此开放真实用户数据库/Vault 读取或迁入入口。** 无产品 UI；真实 DB/授权路径仍阻断。

## 验证

- 定向：`cargo test --lib learning_import` → **51 passed** / 0 failed（含 runner 3 + lifecycle 1 + ops 5 + 既有 import 套件）；日志 `release-artifacts/yingliu-merge/task12d2-target.log`。
- 相关回归：ops/runner/source_profiles/note_origins/source_vault/note_archive/schema_guard → **58 passed**；`task12d2-related.log`。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描 CheckedFiles=564 / PotentialSecrets=0。
- 未重跑全量 Rust/前端（沿用 12c9 记录）；未读取或迁移真实用户数据。

## 边界

- 无 12e UI / 无新 IPC 命令 / 无 push/merge/release/install。
- 真实 Yingliu 用户库与授权路径仍阻断；下步需按计划补 12d 恢复缝（若出现明确 12d3）或继续加固恢复，不得提前做产品界面。
