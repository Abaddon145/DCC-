# Task 13a：迁入批次撤回（归档恢复）

起点 `b5b68e5`。仅合成测试数据，本地提交。

## 改动

- Schema 升到 v35（`schema-v35.sql`，`learning/mod.rs` 迁移，`db.rs` `CURRENT_SCHEMA_VERSION=35`）：
  - `learning_import_reverts`：每批一条撤回记录（原请求摘要、时间、报告 JSON），用于幂等重放。
  - `learning_import_item_reverts`：逐条结果 `reverted / kept_modified / kept_reused / kept_conflict / kept_missing`。
- `import_ops::revert_batch(state, scope, batch_id)`：
  - 只撤回本批 `committed` 且收据 `action=created`、来源映射仍 `Unchanged` 指向同一笔记的条目：
    在同一事务内删除来源映射（`note_origins::forget_note_in`）并把笔记软删除进回收站（`knowledge::trash_in`）。
  - 用户之后改过的笔记 → `kept_modified`；复用早先批次笔记 → `kept_reused`；冲突 → `kept_conflict`；目标不可用 → `kept_missing`。
  - 原始归档（`learning_note_archives`）永不删除，报告带 `archivesKept: true`。
  - 已撤回批次再次调用返回同一份报告；非 `committed` 批次拒绝（「只能撤回已提交的迁入批次」）；跨素材库拒绝。
  - 撤回后把批次 `request_sha256` 改写为带域标签 `yingliu.learning-import.reverted.v1` 的退役摘要，同一来源请求可重新迁入。
- `note_origins::forget_note_in`：仅在事务内、且恰好删除 1 条映射时成功。
- `knowledge::trash_in`：调用方事务内的软删除，等价于 `set_deleted(.., true)`（revision+1、重建索引）。
- 既有测试中「当前版本 = 34」的断言改为 35；`learning_import_ops_upgrade_rolls_back_and_reaches_v34` 模拟 v33 时同时删除 v35 表。

## 测试（先 RED 再 GREEN）

`import_revert_tests.rs`（4 条）：

- `..._trashes_unchanged_created_note_and_keeps_archive`：笔记进回收站、映射清除、归档仍在、同请求可新建 pending 批次。
- `..._keeps_note_edited_after_import`：用户修改后的笔记保留，结果 `kept_modified`。
- `..._keeps_reused_note_from_earlier_batch`：复用笔记保留，结果 `kept_reused`。
- `..._is_idempotent_and_rejects_unfinished_batch`：重复撤回结果一致；pending 批次被拒。

RED：桩实现返回「迁入撤回尚未实现」，4 条全部失败（`release-artifacts/yingliu-merge/task13a-red.log`）。

## 验证

- 撤回专项 `cargo test --lib import_revert_tests`：4 通过。
- 迁入专项 `cargo test --lib learning_import`：83 通过，0 失败（`task13a-green.log`）。
- Rust 全量 `cargo test --lib`：664 通过，0 失败，2 忽略（`task13a-rust-full.log`）。
- `cargo fmt --check`、`git diff --check` 通过。
- 未触碰真实映流 Vault、用户库或付费 AI；未推送。未涉及 Task 14（UI 入口未接）。
