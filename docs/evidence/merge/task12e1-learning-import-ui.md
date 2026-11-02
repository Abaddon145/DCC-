# Task 12e1：学习资料迁入界面（Learning Import UI）

日期：2026-10-29；起点 `8f75b9d`。把 12c/12d 的来源绑定、预检、TaskRegistry 迁入批次接入知识区与任务中心；仍只针对合成/测试数据，不打开真实映流用户 Vault/数据库。

## 范围

- 后端 `learning/import_access.rs`（Windows；非 Windows 走 `import_access_stub.rs` 失败闭合）新增 5 个 Tauri 命令：`bind_learning_import_source`、`preview_learning_import_note`、`queue_learning_import_prepare`、`list_learning_import_batches`、`list_learning_import_items`。全部先核对 `expectedLibraryId` 为当前活动库，库切换后拒绝。
- 绑定只接受用户经系统目录对话框选择的根（`create_vault_source` 身份/epoch），**不从清单提示路径推导授权**；预检复用 `VaultSelection::inspect`（v33 身份 + epoch + 目标冲突），`willOverwrite` 恒为 `false`；转换挂起或缺正文时 `canPrepare=false`，消息“已失败闭合，不能进入暂存”。
- `queue_learning_import_prepare` 在后端重新预检并以 `read_markdown_text` 有界读取（≤5 MiB、读前后长度一致、UTF-8），摘要须与预检一致才构造 `ImportTaskRequest::Prepare` 入队；不一致即拒绝（来源在预检后变化）。
- **通用 IPC 收紧：** `start_library_task` 新增 `ensure_generic_ipc_payload`，拒绝前端直接提交 `LearningImport::Prepare`（其 markdown/摘要由调用方自报）；Commit/Cancel 仍可经任务中心通用入口。前端 `LibraryTaskRequest` 仅声明 `learningImport` 的 commit/cancel。
- 前端 `LearningImportPanel`（知识区侧栏“学习资料迁入”入口，lazy 加载）：展示授权边界、费用边界（仅本机处理，不调用付费 AI/云端，不产生费用）、已绑定来源（库/根/profile/epoch/边界说明）、预检结果（冲突、标题、可准备、会覆盖正文=否、转换挂起、警告）、批次列表与条目（状态/冲突码/目标/暂存路径/回执），以及失败恢复说明与「提交批次」「取消未完成项」（经 `start_library_task` 进入任务中心并轮询结果）。
- 修复上次暂停时的 UI 测试失败：状态栏不再重复预检消息与边界文案（`findByText` 多元素匹配），改为独立的“预检完成：…可/不可排队”提示。

## 验证（Abaddon，Windows）

- 前端定向：`vitest run LearningImportPanel.test.tsx LearningWorkspace.test.tsx` → **2 files / 30 tests passed**；`release-artifacts/yingliu-merge/task12e1-ui.log`。
- 前端全量：`vitest run` → **82 files / 468 tests passed**；`task12e1-frontend-full.log`。
- 构建：`npm run build`（`tsc -b && vite build`）exit 0；`task12e1-build.log`。
- Rust 定向：`cargo test --lib learning_import` → **56 passed / 0 failed**（含 import_access 2、通用 IPC 拒绝 Prepare 1）；`task12e1-target.log`。
- Rust 全量：`cargo test --lib` → **637 passed / 0 failed / 2 ignored**（既有 ignored）；`task12e1-rust-full.log`。
- `cargo fmt --check` 通过；`git diff --check` 通过。

## 边界与残留

- 无真实映流 Vault/SQLite 读取；测试只用 TempDir 合成来源。绑定时 vault 标识固定为 `vault-ui`；多 vault 选择、批量多文件预检属后续。
- 准备任务入队后不在面板内轮询，需在任务中心查看并用「刷新批次」刷新；Commit/Cancel 面板内轮询。
- 未手工桌面验收（Tauri 原生对话框 + 真实窗口）；属 Task 14。
- 无 push / merge / release / install；分支仅本地。
