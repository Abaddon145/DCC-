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

## 9.2 typed 节点生命周期（BASE aee7b74）

**Files:** `learning/note_nodes.rs` / `note_nodes_tests.rs`，`knowledge.rs` / `note_clipboard.rs` / `note_package.rs` / `note_markdown.rs` / `note_export.rs`；`src/lib/learningNoteNodes.tsx` 与测试，现有编辑器、Markdown manager、大纲共享插件注册。

**Interfaces / 决定：**
- `LearningReference { courseId, mediaId, fingerprint, ranges:[{startMs,endMs}], label }`：两个规范 UUID，fingerprint 为当前主文件全 SHA-256，小写 64 hex；1–256 段，整数毫秒，按序且不重叠，0<=start<end<=604800000；label 非空且 <=2000 UTF-8 字节。允许陈旧引用留在正文，点击再检查当前课程/媒体/时长/全文件摘要，不静默改指向。
- `PreservedMarkdown {archiveItemId,label}`：规范 UUID、同样 label 规则；不接受任意路径、HTML 或内嵌伪造正文。保存与粘贴必须确认片段登记存在；归档无删除接口，跨笔记复用同库不可变片段不会随原笔记删除失效。
- `inspect_node(node)->Option<ValidatedLearningNode>` 从 attrs 去掉已有 topicId/topicDepth/topicColor 后严格反序列化，禁止子节点和 marks。复用 `knowledge::inspect_document` 的正文大小、层级与主题规则；时间引用在现有反链表登记为 media 引用，不重建反链系统。
- 归档片段显示由库/笔记作用域命令按 UUID 读取，只有当前笔记或其已保存历史中的片段可查看；前端懒加载只读 textarea、状态与重试，无 innerHTML。
- 共享 ProseMirror filterTransaction 比较前后两个新类型的身份数量，仅实际删除时确认；移动不弹，拒绝删除时正文不变，撤销仍可恢复。无更新的程序化历史展示不弹确认。结构视图已有通用删除提示和插件避免重复确认。
- 笔记包在有归档时用 version2，旧 version1 仍读写兼容。只导出实际引用片段的原字节、hash、size、安全 zip path 与派生来源身份；完整父原件继续由整库备份保留，不能因复制一个块而夹带其他原文段落。导入先全部核验，再复用 archive_bytes/add_archive_item，remap UUID 后走现有 note 事务。原字节发布后若笔记事务失败，归档可作为待处理保留，不把笔记报告为成功。代价：同一父原件的多个片段在包内分别存储，沿用 1000 文件/1GB 包上限。
- Markdown 显式导出用可信登记片段替换 `dccpreserved://<itemId>` 占位，恢复原语法与换行；HTML 转义后输出 pre，TXT 输出原字节解码文本；缺失或损坏片段使导出失败，不导出空白假成功。时间引用导出保留课程/媒体/指纹与全部 range。

- [x] RED→GREEN 后端真实保存/检索/缺引用拒绝/既有回收恢复、剪贴板同库跨笔记、包跨库往返与损坏拒绝、MD/HTML/TXT 输出，旧包仍兼容。命令同主 Task9 的 `learning_note` filter；最终完整 Rust 437 通过，见证据。
- [x] RED→GREEN 前端真实 Tiptap 编辑器：两个节点读写/移动/删除取消与撤销/纯 HTML 不创建特权节点/Markdown 占位不丢、只读查看的加载/错误/重试；旧节点照常。
- [x] 笔记时间跳转命令/播放器入口在 9.3 转换提交前接通：`prepare_learning_note_jump(expectedLibraryId,reference,rangeIndex)`，spawn_blocking 全源校验后返回 courseId/mediaId/positionMs；App 使用现有学习 target/离开保存守卫，首帧 ready 后只执行一次暂停定位。前端字段不是磁盘访问能力。
- 启动命令增加可选 expectedFingerprint，绑定新播放会话；启动原件的现有只读租约再次验证全摘要，避免 prepare/start 间原件替换。旧三参数 Rust QA 入口保留兼容。界面定位按会话首次 ready 执行一次，后续手动 seek 不重置；系列切到其他课程不套用旧引用指纹。
- 大纲/导图的内嵌 BlockEditor 也复用节点，但删除统一在主正文事务确认一次；主正文拒绝时内嵌内容立即恢复，不保留看似删掉的界面草稿。
- [x] 完整回归/构建/证据/本地提交，不标 Task9 全部完成；最终 437 Rust / 376 前端通过，生产构建成功。证据 `docs/evidence/merge/task9-note-nodes.md`；实际 IME/DPI 与真实迁入仍单独验收。

## Review Focus

- 损坏同名归档不能被自动覆盖，重复导入不能返回假成功（9.1 损坏/去重测试）。
- 外层数据库事务回滚不能留下声称成功的文件（9.1 SQL 失败/外层事务测试）。
- 特殊内容删除/包导出/剪贴板不能只保留一个悬空 UUID（9.2–9.4 端到端测试）。
- UTF-8 字节偏移、BOM 与 CRLF 不可被 JS 字符索引悄悄改写（9.1 范围测试、9.3 跨语言 fixture）。
- 切库、原件变化、未发布附件不可把旧预览提交给新库（9.3 守卫、Task 12 只读快照）。
