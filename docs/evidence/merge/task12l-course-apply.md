# 课程批次暂存与提交

2026-10-29。接在 `425751e`（PR #19 的 squash）之后。课程批次不再只是日志身份：内部函数可以把已经算好的课程预检暂存进 schema v37 日志，再在合成库上提交或取消。没有 Tauri 命令，也没有界面。

## 行为

- 沿用 `learning_import_batches.batch_kind='courses'`。schema v37 新增 `learning_course_import_stage` 和 `learning_course_import_entries`。笔记条目表的 `target_kind` 仍只允许 `note`，课程决策不进笔记暂存。
- `stage_course_batch` 用批次上的来源身份重新预检，把绑定、指纹、系列、课程、冲突和来源摘要写成一份规范 JSON。同一份内容可以再次暂存；内容不同则拒绝，直到取消。
- `commit_course_batch` 先对照暂存指纹。来源或目标指纹变了就失败，暂存还在，目标库不写。指纹一致后锁定批次行，再核对目标课程和来源映射，然后在同一个事务里写入。
- 只有来源映射仍是未映射、媒体已解析、并且没有共享媒体或来源进度冲突时才新建课程。进度按来源行原样写入，不取最大值。已有进度与来源一致时只登记来源，不改修订号，也不改已有标题。
- 来源没有进度行时不新建课程。表上的 `position_ms` 不能表示“没有进度”，新建行会变成 0，所以缺失和明确的 0 分开：明确的 0 会写入，缺失不建课。已有课程如果来源没有进度，保留原进度并登记来源。
- 目标进度与来源不一致、来源摘要已变、目标已变、目标不可用、共享媒体或同系列重复成员，都留下冲突回执，不覆盖进度，也不把多个来源课次合成一门课。回执保留来源位置和目标位置。
- 一个系列里只要有课次不能在本次全部新映射，就不写系列成员，系列回执是 `membership_withheld`。能单独映射的课次仍然提交。全部新映射时按来源顺序写入成员，并登记系列来源。来源系列 ID 和课次 ID 留在来源键和日志的 `source_id` 里；目标课程 ID 仍是新的 UUID。
- 已提交的批次再次调用提交只返回原回执，不按后来的来源进度重写。
- `cancel_course_batch` 删除暂存并把批次标为已取消，不改目标和笔记批次。笔记批次不能走课程操作，课程批次也不能走笔记的添加、暂存、提交、取消或撤回。
- v37 升级失败时版本停在 36，两张新表不在；成功后笔记批次仍是 `notes`，课程批次仍是 `courses`。

## 本机验证

工具链 `rustc 1.89.0` / `cargo 1.89.0`。命令是 `cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib <filter> -- --test-threads=1`。合成临时库。

- 第一次 `learning_import_course_apply`：4 通过，1 失败。失败点是来源进度改成 99999 之后，回执写成了 `divergent_progress`，目标 22000 没有被覆盖。随后让来源映射状态优先于单纯的进度差异，同一测试改为 `source_changed`，目标进度和修订号仍不变。
- 修正后 `learning_import_course_apply`：5 通过。
- `learning_import`：92 通过，0 失败。含这 5 个新测试、课程批次隔离、课程来源、课程预检，以及原有笔记迁入。
- `upgrade_schema_guard`：14 通过。
- 抽查升级后版本为 37 的用例：笔记来源 v32、来源档案 v33、来源身份 v30、笔记归档 v26、笔记预检 v27 与备份、摘录 v28、课程分析 v25、旧库打开和分享链接迁移，均通过。
- `cargo +1.89.0 fmt --all -- --check` 和 `git diff --check` 通过。

## 非声明

- 没有课程撤回，没有课程批次列表或恢复界面，没有 Tauri 命令。
- 没有校验媒体文件身份，也没有授权读取真实映流库。绑定仍由调用方传入。来源库和目标库不是同一个原子快照；提交用暂存指纹拒绝过期预检，并在目标事务里再读目标行。
- 没有把来源的最后播放时间、更新时间或会话序号写入目标。没有改笔记迁入的写入规则。
- 系列只在本次每个课次都能新映射时整组写入。不会为了保留半套顺序去改写已有系列。
- 没有做笔记那种中途失败注入。课程提交包在一个 SQLite 事务里，失败会整笔回滚并保留暂存。
- 没有真实映流 Vault、用户数据库、Cookie 或凭据。没有安装、NSIS、发布或合并到 main。
- Linux 上 `learning_baseline_two_libraries_preserve_scoped_identities_and_media_hash` 仍因「无法定位本机访问批准」失败。`acceptance_asset_network_backup_restores_identity_analysis_and_soft_delete` 已通过 schema 37 断言，随后在 `restored[0].available` 失败。这两处都不是课程日志的版本断言失败。Windows 全量和带 `cfg(windows)` 的 WAL 预检没有在这台机器上跑。
