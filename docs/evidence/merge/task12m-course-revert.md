# 课程批次撤回

2026-10-29。接在 `99c89a4`（PR #20 的 squash）之后。已提交的 `courses` 批次可以在内部撤回。没有新表，没有 Tauri 命令，也没有界面。

## 行为

- `course_apply::revert_course_batch(state, scope, expected_epoch, batch_id)` 只处理 `batch_kind='courses'` 且状态为 `committed` 的批次。笔记批次返回「笔记迁入批次不能使用课程操作」。pending 和 cancelled 返回「只能撤回已提交的迁入批次」。课程提交是一整笔事务，取消后没有已提交条目。
- 调用方必须带上当前来源 epoch。epoch 不是合法 UUID，或与来源档案的当前 epoch 不一致，就拒绝。已有撤回报告时，过期 epoch 也不能读出旧报告。
- 幂等记录复用 `learning_import_reverts`。课程条目不在 `learning_import_items` 里，所以不写 `learning_import_item_reverts`；逐条 `reverted / kept_modified / kept_reused / kept_conflict / kept_missing` 放在同一份报告 JSON 里。再次撤回返回同一份报告，不重复删除。
- 只有本批 `committed` 且回执 `action=created`、来源映射仍是 `Unchanged` 并指向同一目标的课程或系列才会删除。删除前先在同一事务里撤销这一条来源映射（`course_origins::forget_in` / `forget_series_in`），并且必须恰好删到一行。
- 其他已提交批次如果也指向同一目标，结果是 `kept_reused`。用户后来改过进度、标题或系列修订，映射不再是 `Unchanged`，结果是 `kept_modified`。冲突是 `kept_conflict`。目标行已经不在、映射还在，结果是 `kept_missing`，来源行保留。
- `action` 不是 `created` 的已提交条目（已有课程只做映射，或沿用早先映射）不删目标，也不删来源行，结果是 `kept_reused`。
- 课程若挂了笔记、字幕、分析、分析预览或摘录，或者还属于本次不会删除的系列，就按 `kept_modified` 留下。本次自己新建、并且这次会一起撤回的系列成员关系不算额外使用。
- 系列本身没被改过时，可以删掉这个系列。成员课程如果用户改过进度，课程和进度留下，只去掉本次新建的系列。系列被改名或修订号变化时，系列和其中的课程都留下。
- 撤回不打开来源库，不改媒体行，不删 `learning_note_archives`。报告带 `archivesKept: true`。
- 成功后把批次 `request_sha256` 改成带域标签 `yingliu.learning-course-import.reverted.v1` 的退役摘要。批次状态仍是 `committed`。同一来源请求可以再开一个课程批次。笔记撤回仍使用原来的域标签，并且仍然拒绝课程批次。

## 本机验证

工具链 `rustc 1.89.0` / `cargo 1.89.0`。命令是 `cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib <filter> -- --test-threads=1`。合成临时库。

- 先放桩实现「课程撤回尚未实现」。`learning_import_course_revert`：0 通过，7 失败。失败点都是这个错误，或把它和「只能撤回已提交的迁入批次」比较失败。
- 实现后同一过滤器：7 通过。覆盖未改动的新建课程和系列被删除、媒体和来源文件字节保留、原文归档计数不变、重复撤回结果相同、请求摘要可再次占用；用户改过的进度留下而系列被收回；改过的系列和课次都留下；后一批复用时两边都是 `kept_reused`；已有课程的映射和冲突进度不被动；目标课程被删掉后是 `kept_missing` 且来源行还在；笔记关联和摘录会保住课程；笔记批次、未提交批次、已取消批次和过期 epoch 被拒绝；笔记撤回仍把未改动笔记送进回收站，旁边的课程行还在。
- `learning_import`：99 通过，0 失败。含这 7 条、课程暂存提交取消、课程批次隔离，以及原有笔记迁入和笔记撤回。
- `cargo +1.89.0 fmt --all --manifest-path src-tauri/Cargo.toml -- --check` 和 `git diff --check` 通过。

## 非声明

- 没有课程批次列表、恢复界面或 Tauri 命令。
- 没有新 schema。课程条目的撤回结果只在批次报告里，不另建条目表。
- 没有把已有课程的进度改回迁入前的值，也没有软删除课程；能撤回的新建课程是整行删除，媒体不删。
- 没有改笔记迁入、笔记撤回或外部视频归档的写入规则。
- 没有真实映流 Vault、用户数据库、Cookie 或凭据。没有安装、NSIS、发布或合并到 main。
