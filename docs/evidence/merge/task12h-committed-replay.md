# Task 12h：已提交批次同请求重放与请求摘要加固

起点 `c86f94b`。

## 改动

- `import_access.rs`：新增 `import_request_sha256`，把请求身份改为带域标签的 JSON 数组
  `["yingliu.learning-import.prepare.v2", profile_id, epoch, vault, relative_path, source_sha256, target_token]`
  再做 SHA-256。旧实现以 `|` 拼接且不含目标令牌，vault/路径含 `|` 时可撞车（`v|a`+`b.md` 与 `v`+`a|b.md`）。
- `prepare_payload` 用 `preview.target_token` 参与请求摘要，因此 `tasks.rs` 的 `submit_scoped_learning_import`
  所校验的目标令牌也已固化到任务载荷的 `request_sha256` 中（载荷结构不变，未新增字段，旧持久化任务仍可反序列化）。
- `import_runner.rs` Prepare：`create_batch` 幂等返回 `committed` 批次时，直接返回
  `{batchId, status:"committed", receipt}`，不再 add_item / stage；`cancelled` 等其他非 pending 状态仍报错
  （`create_batch` 本身只复用 pending/committed，cancelled 会新建批次）。

## 测试（先 RED 再 GREEN）

- `learning_import_runner_replays_committed_batch_receipt_without_restaging`：提交后同请求重放拿回原批次与原收据，批次条目数仍为 1。
  RED：原实现报「迁入批次已结束，请使用新的请求摘要」（`release-artifacts/yingliu-merge/task12h-red-runtime.log`）。
- `learning_import_request_hash_binds_target_token`：不同目标令牌 → 摘要不同；同输入稳定。
- `learning_import_request_hash_does_not_collide_on_pipes`：vault/路径含竖线不撞车。
- `learning_import_prepared_request_hash_includes_preview_target_token`：`prepare_payload` 产出的摘要等于含预览目标令牌的新摘要。
  上述三条 RED 为编译失败（函数不存在，`task12h-red.log`）。

## 验证

- 迁入专项 `cargo test --lib learning_import`：79 通过，0 失败（`task12h-green.log`）。
- Rust 全量 `cargo test --lib`：660 通过，0 失败，2 忽略（`task12h-rust-full.log`）。
- `cargo fmt --check`、`git diff --check` 通过。
- 仅使用合成临时目录数据；未触碰真实映流 Vault、用户库或付费 AI；未推送。