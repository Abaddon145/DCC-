# Task 9：唯一笔记正文与原字节归档实施细化

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. 按主计划已批准的顺序执行；本文件细化 Task 9，不新增产品范围。

**Goal:** 保留映流笔记的真实原字节，再转换到栈藏现有 `document_json`；特殊内容与时间引用不丢失。

**Architecture:** 复用知识库编辑器、MarkdownManager、附件、revision、备份和任务。原字节只读归档是核对资料，不是另一套编辑正文；不引入第二套 Markdown 解析器或 Vault 同步。

**Tech Stack:** 现有 Rust/rusqlite/tempfile/SHA-256，React/Tiptap MarkdownManager。

**Spec:** `C:/Users/14582/Documents/ChatGPT/视频下载软件/docs/superpowers/specs/2026-10-21-stackdcc-yingliu-merge-design.md`，主计划 `2026-10-21-stackdcc-yingliu-merge.md` Task 9；已冻结来源 `94ee1e8` 的 `docs/integration/yingliu-migration-input-v1.md`。

## 约束与现状

- BASE `7fdf5c9`；保留 v0.23 全部功能，不修改源仓库/用户 Vault/真实数据，不合并发布。
- 源 Markdown/history 单项上限 5 MiB；目标 JSON 保持现有 4 MiB，不截断越界内容。无效编码也先保留字节，转换仍 pending。
- 源身份是 profile + sourceKey + versionKey，不能凭标题/内容合并笔记。归档行的不同内容摘要保留多个版本，不覆盖。
- 现有前端 `parseNoteMarkdown` 拒绝嵌入 HTML，不能直接当无损转换器；现有 Rust importer 不保留原字节，既有附件归属于 noteId。
- `learning-data` 已进入完整备份/恢复；新归档复用其路径验证，文件不通过任意前端路径读取。
- 真实源读取/快照及 profile 映射在 Task 12；本阶段只接受调用方已经核验的有界字节，不读取源数据库。

## 支持矩阵（后续转换接入验收依据）

| 来源 | 目标与保护 |
| --- | --- |
| 标题/段落/标准列表/待办/表格/代码 | 复用 MarkdownManager 生成可编辑节点；代码中的 HTML 只是文本 |
| 已发布的安全图片/附件 | 使用目标 noteId 所属附件 UUID；未成功发布则 pending，不能标完整 |
| 有映射且时间有效的 yingliu 视频引用 | learningReference，保留多个非连续 range；身份与指纹后台核验 |
| 未知课程/越界时间/缺附件 | 原字节归档 + 明确待处理/失效提示，不猜同名目标 |
| BOM/CRLF/frontmatter/Wiki/callout/注释/数学/未知 HTML | 归档保留字节；暂不支持部分使用 preservedMarkdown，可读纯文本，不执行 HTML |
| 不合法 UTF-8/目标 JSON 超限 | 归档可核对/导出，转换 pending，不能伪造成功正文 |

## 9.1 原字节归档（当前执行）

**Files:** 新 `learning/note_archive.rs`、`learning/note_archive_tests.rs`、`learning/schema-v27.sql`；修改 `learning/mod.rs`。

**Interfaces:**
- `ArchiveSource { profile_id, source_key, version_key }`，profile 为 UUID；两个来源键非空、最多 4096/300 UTF-8 字节，无控制字符，仅身份不作为路径。
- `archive_bytes(db, base, source, bytes, expected_sha256) -> NoteArchive { id, sha256, size }`：后台入口，不注册 Tauri 命令。允许空文件，最多 5 MiB。调用前完整摘要不匹配直接拒绝。同身份/版本/摘要返回原 ID，但必须重新校验已存文件。不同版本或摘要保留独立行。
- `read_archive(db, base, archive_id) -> Vec<u8>`：仅已登记 UUID，固定 `learning-data/archives/<id>.bin` 路径；校验大小/全摘要、拒绝 reparse、路径篡改、非普通文件。
- `add_archive_item(db, base, archive_id, start_byte, end_byte, label) -> ArchiveItem { id, archive_id, start_byte, end_byte, sha256, label }`；非空合法字节范围，端点不切开 UTF-8 字符；label <= 2000 字节且无控制字符；相同范围重用 ID。原件不是有效 UTF-8 时保留整个归档，不能生成误解码块。
- `read_archive_item(db, base, item_id) -> Vec<u8>`：重新校验父归档及片段摘要，仅输出精确原字节，不转换换行/编码。
- schema v27 两张表：`learning_note_archives` 存来源身份、hash、relative_path、size、created_at；`learning_note_archive_items` 存父归档、范围、hash、label。无旧 JSON 改写、无外部路径、无删除 API。

- [x] RED：真实临时文件/SQLite 测试 BOM/CRLF/HTML 原字节与片段、600 KiB 往返；同源去重/源变化保留双方/跨 profile 不合并；hash 不符/超限/路径篡改/损坏/跨库/reparse/越界 UTF-8；SQL 失败不留本次文件；v26->v27 幂等及完整备份恢复。
- [x] GREEN：独占新文件、临时文件 fsync + persist_noclobber，数据库事务发布。拒绝在调用方已有事务内发布原文件，避免外层 rollback 遗留孤儿；片段只写数据库可参加后续事务。文件先发布时崩溃可能遗留未登记 UUID 文件，不作为有效归档读取；Task 12 恢复报告处理，不擅自清理。
- [x] 运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_note -- --test-threads=1`，然后完整 Rust/前端/生产构建；记录证据、本地独立提交，不将 9.1 称为整项 Task 9 完成。最终 426 Rust / 365 前端通过，生产构建成功，见 `docs/evidence/merge/task9-note-archives.md`。

## 后续顺序（进入各步前核对具体调用链与扩展接口）

1. 9.2 两个 typed 节点：后端白名单/搜索/反链、归档项归属；编辑器/大纲/导图、移动/删除确认、安全粘贴。先全链路测试，不能先启用前端后让保存失败。
2. 9.3 转换预览/提交：复用现有 lexer/MarkdownManager；后台原字节范围绑定、映射校验、pending 与 expectedLibraryId/revision 守卫，不直接信任前端正文/摘要。精确分段接口在阅读现有 lexer 实际 token 行为后固定，禁止自己写简陋 Markdown parser。
3. 9.4 笔记包/显式 Markdown/HTML/TXT 导出与恢复：原字节/块引用随包复制，保留时间与来源；跨库引用不凭 UUID 假装可用；不修改旧 JSON。
4. 全 Task 9 回归/600 KiB 实际编辑 I/O；IME/撤销/失败草稿/物理显示在真实测试中分别报告，不能以纯函数模拟冒充完成。

## Review Focus

- 损坏同名归档不能被自动覆盖，重复导入不能返回假成功（9.1 损坏/去重测试）。
- 外层数据库事务回滚不能留下声称成功的文件（9.1 SQL 失败/外层事务测试）。
- 特殊内容删除/包导出/剪贴板不能只保留一个悬空 UUID（9.2–9.4 端到端测试）。
- UTF-8 字节偏移、BOM 与 CRLF 不可被 JS 字符索引悄悄改写（9.1 范围测试、9.3 跨语言 fixture）。
- 切库、原件变化、未发布附件不可把旧预览提交给新库（9.3 守卫、Task 12 只读快照）。
