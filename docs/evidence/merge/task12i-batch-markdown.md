# Task 12i：已授权 Vault 内的多篇 Markdown 迁入批次

2026-10-29。在 `codex/yingliu-learning-integration` 上把生产迁入从「一次一篇 Markdown」扩成「同一已授权目录中的一批 Markdown」。未读取真实映流 Vault、用户库、cookie 或凭据；未调用付费 AI；未安装、打包、推送 main、合并或发布。课程、系列和 SQLite 源库迁入不在本次范围。未依赖、也未合并 PR #17 的归档工作。

## 行为

- 目录授权仍只走现有的系统目录对话框和会话绑定。界面 Vault 标识仍是 `vault-ui`。前端不提交授权根之外的绝对路径。
- 单篇路径保持原样：预检后「排队准备」仍调用 `queue_learning_import_prepare`。一篇笔记的请求摘要仍是 `yingliu.learning-import.prepare.v2`，任务载荷仍用原来的单篇字段，`items` 为空且不写入 JSON，以便已排队的旧准备任务还能反序列化。
- 多篇路径复用同一条 Prepare：`pick_learning_import_notes` 在已授权根内多选 Markdown，或把多次预检的相对路径加入同一列表。`queue_learning_import_prepare_batch` 重新预检、有界读取，并按相对路径排序后放进**一个** pending 批次。两篇及以上的请求摘要是 `yingliu.learning-import.prepare.v3`，提交时用各篇目标令牌的组合摘要再核对一次。
- 上限：最多 32 篇，每篇仍不超过 5 MiB，整批原文合计不超过 20 MiB。重复路径（大小写不敏感）在入队前拒绝。
- `willOverwrite` 仍恒为 false。冲突项可以暂存并在提交时留下回执，不改已有笔记正文。提交、取消、撤回仍走现有 TaskRegistry 和批次界面。
- 含 `..`、空段或越出授权根的选择在准备前失败，不创建批次。多选里只要有一篇在授权根外，整次选择失败，不返回部分列表。

## RED

先让多篇准备在校验处失败闭合（「迁入准备一次只能处理一篇 Markdown」），再跑：

`cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_runner_ -- --test-threads=1`

多篇准备、冲突批次、取消/提交/撤回批次都在入队时得到该错误。越界用例期望「笔记来源身份无效」，实际仍是上述单篇闸门。这是实现前的失败，不是最终行为。

同一次运行里，既有的提交类用例在 Linux 上还会碰到原文归档的 `ensure_space`（非 Windows 生产构建返回「下载功能当前仅支持 Windows」）。测试构建随后为 `cfg(test)` 增加了 `statvfs` 可用空间检查，生产非 Windows 构建仍失败闭合。

## GREEN（本机 Linux，合成临时目录）

工具链 `rustc 1.89.0`，与 `src-tauri/Cargo.toml` 的 `rust-version` 一致。本环境自带的 1.83 无法解析锁文件里的 edition 2024 依赖。

`RUSTUP_TOOLCHAIN=1.89.0 node scripts/qa/verify-dcc-learning-migration.mjs`

```text
{"scope":"synthetic Rust and React tests; not desktop or real Vault acceptance","passed":{"rust":65,"frontend":19}}
```

日志：`release-artifacts/yingliu-merge/migration-qa-v9IVm3/`（不进 Git）。

- Rust：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import -- --test-threads=1` → `test result: ok. 65 passed; 0 failed; 0 ignored`。其中包含多篇准备同一批次、越界相对路径拒绝、大小写重复拒绝、冲突项不覆盖正文、单篇准备仍只有一条、多篇批次的取消/提交/撤回。
- 前端：`vitest run --configLoader runner src/features/learning/LearningImportPanel.test.tsx` → 1 file / 19 passed。覆盖单篇排队仍走原命令，以及多选/加入本批后一次 `queueImportPrepareBatch`。
- `npx tsc -b --pretty false` 退出 0。
- `cargo +1.89.0 fmt --check` 与 `git diff --check` 退出 0。

`import_access` 与 `source_markdown` 仍是 `cfg(windows)`。本次新增的多选、授权根外文件拒绝、以及经真实预检后的批次准备测试写在 `import_access_tests.rs`，本 Linux 代理没有执行。尝试 `cargo check --tests --target x86_64-pc-windows-gnu` 时停在依赖 `aws-lc-sys` 的构建脚本，没有编到迁入代码。因此这 65 项不能代替 Windows 上的目录租约与原生多选对话框验收。

为让 Linux 上的 `cargo test --lib` 能编过，两则本来就只调用 Windows API 的进程测试加上了 `#[cfg(windows)]`（`learning_native_process_drop_reaps_owned_tree`、`download_cancel_terminates_grandchildren`）。它们的行为没有改。

## 明确不做

- 不打开真实映流 Vault、真实用户库或源 SQLite/WAL。
- 不迁入课程、系列、进度或源数据库。
- 不做桌面窗口、中文输入法或物理 DPI 验收。
- 不调用付费 AI，不安装，不打 NSIS，不合并到 main。
