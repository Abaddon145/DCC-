# Task 12d3：迁入批次中途失败恢复

日期：2026-10-28；起点 `4c90ea5`。加固 schema34 `import_ops` 在逐项提交中断后的恢复：可续跑未完成批次、可取消剩余项并清理 `.dcc/runtime/imports/` 暂存；已成功提交的笔记保留且不覆盖正文。无 Tauri/UI/IPC，无真实映流路径。

- `commit_batch`：每项提交/冲突后立即清理该条 runtime 暂存；续跑时计入已终态条目进 receipt，并修复遗留 stage 指针；测试注入 `set_commit_fail_after_applies` 证明中途失败闭合。
- `cancel_batch`：保留已 `committed`/`conflict` 条目与笔记；取消未完成项；清理含孤儿在内的全部 staging 并尝试删除空批次目录。
- 新增 `list_items` 供恢复检视批次日记。
- **不能因此开放真实用户数据库/Vault 或 12e UI。**

## 验证

- 定向：`cargo test --lib learning_import` → **53 passed**（ops 7，含 mid-resume / mid-cancel）；`task12d3-target.log`。
- 相关：ops/runner/profiles/origins/vault/archive/schema_guard → **60 passed**；`task12d3-related.log`。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描 CheckedFiles=565 / PotentialSecrets=0。
- 未重跑全量 Rust/前端；未读取真实用户数据。

## 边界

- 无产品 UI / 无新 IPC；真实 Yingliu 库仍阻断。计划无即时 12d4 勾选；下步另开恢复/撤回缝或 12e（需单独授权）。