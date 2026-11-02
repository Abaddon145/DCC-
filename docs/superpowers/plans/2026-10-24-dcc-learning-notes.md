# Task 9：唯一笔记正文与原字节归档实施细化

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. 按主计划已批准的顺序执行；本文件细化 Task 9，不新增产品范围。

**Goal:** 保留映流笔记的真实原字节，再转换到栈藏现有 `document_json`；特殊内容与时间引用不丢失。

**Architecture:** 复用知识库编辑器、附件、revision、备份和任务。现有手动 Markdown 入口保留 MarkdownManager；后台迁入用固定版本 Rust CommonMark 解析器直接生成同一 `document_json`，不能依赖编辑器窗口运行。原字节归档只用于核对，不新增正文编辑系统或 Vault 同步。

**Tech Stack:** 现有 Rust/rusqlite/tempfile/SHA-256，React/Tiptap MarkdownManager；后台解析使用锁定的 pulldown-cmark 0.13.5（9.3a 决定）。

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
| 标题/段落/标准列表/待办/表格/代码 | 手动 MarkdownManager 与后台固定 Rust 解析器生成同一可编辑节点；共享 fixture 核对语义，代码中的 HTML 只是文本 |
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
2. 9.3 转换预览/提交：已验证现有 lexer/MarkdownManager 的能力与边界；后台转换不能依赖 WebView，使用固定版本 Rust 解析器。原字节范围绑定、映射校验、pending 与 expectedLibraryId/revision 守卫，不直接信任前端正文/摘要；禁止自行编写简陋 Markdown parser。
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

## 9.3a 后台纯转换器（BASE 0239e21）

**Ruling（替代早期“只复用 JS parser”的内部选择）：** Task 12 是 Rust 后台迁入任务；依赖 WebView 执行 JS 转换会在窗口关闭/重启时失去必要能力。没有可复用 Rust Markdown 依赖，自写解析器更难保证格式与偏移。新增固定 `pulldown-cmark = {version="=0.13.5",default-features=false}`，锁定 Cargo.lock；不启用 HTML 渲染、CLI 或 SIMD，不新增编辑器。代价是 Rust/JS 两个导入入口需共享常用格式 fixture 验证语义，默认正文仍唯一为 document_json。

**来源：** 官方仓库提交 `48a2767b5b8ab0a4893e80d942e510065889fc6b` 的 `pulldown-cmark/Cargo.toml` 与 `LICENSE` 已用 GitHub connector 核对，版本 0.13.5、MIT、最低 Rust 1.71.1。Cargo 配置镜像成功获取同版本（官方 crates HTTP 被拒绝后按正常 Cargo 源替换查询），不用无法打开的网页缓存冒充验证。实际依赖及所有新增传递依赖许可随资源打包。

**Files:** 新 `learning/note_conversion.rs` / `note_conversion_tests.rs`；改 Cargo.toml/lock、learning/mod.rs、第三方声明及资源列表。该纯边界完成前不替换现有手动 MD 导入。

**Interfaces：**
- `AttachmentTarget {attachment_id:String, kind:Image|File}`；`AttachmentMap=HashMap<sourceKey,AttachmentTarget>`。只接受明确映射，不按文件名猜测；函数不读取路径或下载文件。
- `MediaTarget {course_id,media_id,fingerprint,duration_ms}`；`MediaMap=HashMap<sourceItemId,MediaTarget>`。旧 `yingliu://video/<id>?t=<ms>` 映射明确的时间点为 `[t,t+1)`；越界/未知身份保留原文并 pending，不虚构连续片段。多 range 由已有 typed 节点/源记录适配，不合并成一个跨度。
- `convert_source_note(bytes:&[u8], attachments:&AttachmentMap, media:&MediaMap)->Result<ConversionPreview,String>`，纯 CPU/内存，无文件/网络/数据库/付费调用。输入 <=5MiB；返回 `document:Option<Value>`, `archive_hash`, `warnings`, `unsupported:Vec<PreservedSpan>`, `pending:bool`。
- `PreservedSpan {placeholder_id,start_byte,end_byte,label,reason}`：规范 UUID 仅为草案占位；范围直接来自解析器原始 UTF-8 字节偏移，包含原换行与特殊标记；9.3b 在已登记原件上创建实际片段并 remap。暂存 UUID 不能绕过现有 save_in 的归档登记校验。
- 常用标题/段落/引用块/列表/任务/无特殊属性表格/代码/HTTP 链接直接转现有节点。图片/本地附件只有映射明确时产生托管 UUID 节点；缺附件保留块并 pending。Code 中 HTML 为普通代码，实际 HTML/注释/frontmatter/未支持 Wiki/公式/脚注等按完整最外层块保留，不能执行或静默删掉。
- 原件摘要对全部原字节计算。BOM 与空白/源定义即使未成为编辑内容，也必须保留在原始归档；非空无法说明的解析间隙生成保留块。无效 UTF-8 返回 document=None/pending，允许调用方保留/导出原字节，不伪装转成字符串。
- 输入超过5MiB直接拒绝；结构超过50,000行、100,000事件、64层或目标JSON超过4MiB时，以整段原文保留并 pending，不截断或无界构建。普通 600KiB 文本必须实际转换并通过知识文档校验。

- [x] RED：标准格式/附件与课程映射、未知 HTML/Wiki/frontmatter/公式原始范围、BOM/CRLF/emoji、缺附件/越界课程、跨段引用定义、600KiB、无效编码与超限，转换桩 7 组全部失败；再添加链接规范化、嵌套与围栏属性边界，先失败再修复。
- [x] GREEN：启用固定原生解析器 offset iterator，流式组装现有节点，保留未知整块；不调用 HTML renderer。9 组转换定向通过，4 组常用格式 fixture 在 Rust/JS 共用，前端 Markdown 10 项通过。结果经现有 inspect_document 校验，归档占位待 9.3b remap 后才能保存。
- [x] 核对锁文件最小增量、第三方许可；定向测试、完整 Rust 446 / 前端 380 通过、生产构建成功。证据 `docs/evidence/merge/task9-note-conversion.md`，本地独立提交；此步未接通预览/提交或实际迁入。

9.3b 再固定库/来源身份、预览快照、附件原子发布、目标 revision、幂等与状态报告接口，并在启用入口前测试取消/切库/失败恢复；不能把纯转换器的模拟通过当成真实迁入成功。

## 9.3b1 持久预览与事务提交内核（BASE 7ac1644）

**Files:** 新 `learning/note_preview.rs` / `note_preview_tests.rs`、`schema-v28.sql`；修改 learning/mod、纯转换 DTO serde、knowledge_files 的格式查询可见性及现有 schema 版本断言。命令/UI 在内核通过后接通，不先开放接受任意路径的前端接口。

**Interfaces / 决定：**
- `Destination::New{title,notebook_id}` 或 `Replace{note_id,expected_revision}`。Replace 必须显式选择，保持既有笔记元数据，保存再次核对修订；冲突留原文和预览，不覆盖用户编辑。需要另存时生成 New 预览，不自动作此决定。
- `AttachmentSource{source_key,path,sha256}` 仅为 Rust 内部已批准来源参数，不反序列化为前端文件权限。最多1000个、单项100MiB、合计1GiB，规范键<=4096字节；先复制到本库 `learning-data/note-previews/<previewId>/<uuid>.<ext>` 并核对完整摘要。重解析路径拒绝，磁盘空间复用 runtime/tools::ensure_space；格式复用 knowledge_files。未知/缺失附件不传映射，转换器生成 pending 原文。
- `CourseMap=HashMap<sourceKey,courseId>` 为后台明确来源映射。读取实际课程/媒体/主文件/时长与完整摘要、现有本机授权；只读租约保持到相应操作结束。提交再次验证当前映射、所有引用范围，不能接受前端自报 fingerprint。
- `prepare_note_preview(db,base,libraryId,archiveId,destination,sources,courses,grants)->StoredNotePreview`：只读已登记原件。保存有界快照（<=16MiB）及附件副本，不创建笔记。原件完整SHA、库ID与规范位置摘要、目标修订、明确映射和快照摘要一起绑定；相同身份/材料与目标复用非取消预览。
- schema28 单表 `learning_note_previews` 保存 UUID、归档ID、身份摘要、快照JSON及SHA、ready/committed/cancelled 状态、结果回执和时间。预览不是正文；结果仍唯一写现有 knowledge_notes/revisions/attachments/search。无需单独任务队列或第二笔记系统。
- `commit_note_preview(db,base,libraryId,previewId,grants,finalize)->NoteCommitReceipt`。后台连接校验原件与附件后进入 IMMEDIATE 事务，再读取状态；已提交只返回原回执，即使用户后来编辑也不再次写入。取消不可提交。原文片段在同一事务登记并 remap，附件复用既有 knowledge_files::import，笔记复用 save_in；失败回滚且只清理本次新发布附件。保留既有附件与所有原文归档。
- `finalize(Transaction)->Result` 是原子最终门：生产调用在 AppState.with_library_if_id 的短锁中核对库/位置/维护状态并 commit；大量复制与完整摘要不持有全局 AppState 锁。外层迁入 TaskRegistry 的任务租约/取消仍由 Task12 负责，本内核不另建调度系统。
- `cancel_note_preview` 记录 cancelled 并只清理该预览登记的副本；提交后不可撤销为取消。清理失败可重试，警告保留，不将已落库笔记伪装成失败。无效编码不生成空白成功笔记；pending 正文与成功回执分别显示，不把部分转换报告为完整迁入。
- 来源档案/逐项迁入映射继续由 Task12 的 learning_origin_map 管理，本表只负责用户确认的不可变转换快照与幂等回执，不以标题或内容摘要跨来源合并。

- [x] RED：4 组预览/提交桩全部失败；追加回执丢失/坏图片与批量片段、备份恢复先失败再实现。真实临时库覆盖上述边界，最终 11 组通过。最终门拒绝为可控事务回调模拟，真实 AppState 命令适配留 9.3b2。
- [x] GREEN：持久快照、独占副本、源/库/修订验证、单事务保存/回执，失败恢复与精确清理；v27->v28 幂等并保留旧正文/归档。批量片段仅读父原件/验证 UTF-8 一次，savepoint 覆盖整批；恢复备份撤销 ready 预览，保留原件及已提交正文。
- [x] 定向 39 项、完整 Rust 457 / 前端 380 通过、生产构建成功，凭据扫描与本地阶段提交。证据 `docs/evidence/merge/task9-note-preview-core.md`。真实迁入、命令/UI激活、旧历史浏览/计数仍后续完成。

**9.3b2 接续前核对：** 原子 AppState 库/位置/维护最终门，耗时 IO 的可取消上下文；前端只给归档/附件 UUID，不开放路径；保留注册附件原文件名；重复请求/并发提交重读持久回执；未发布图片须使用受控预览读取能力，不能把占位 UUID 当已登记附件；恢复/迁移后的历史原文浏览与重新预览入口独立于旧批准。

## 9.3b2 受控命令与运行时守卫（BASE 1b20617）

**Files:** 新 `learning/note_preview_runtime.rs` / `note_preview_runtime_tests.rs`，修改 core 的取消上下文与来源显示名、mod/lib 命令注册；`src/features/learning/types.ts` / `api.ts` 及 API 路由测试。暂不开放编辑器预览页面，受控预览图片读取与历史 UI 在后续完成后再激活。

**Interfaces：**
- `RegisteredAttachment {sourceKey,attachmentId}`、`PrepareNotePreviewRequest {archiveId,destination,attachments,courses}` 严格 camelCase/deny_unknown_fields。前端只能提供本库登记 UUID；后台查询真实 attachment path/hash/原文件名，不接受 raw path、document 或 fingerprint。
- Runtime `prepare/get/commit/cancel_in_library(state,expectedLibraryId,...)` 先绑定库目录，再用现有 worker connection 在全局锁外执行 IO。提交最终门在 `with_library_if_id` 短锁中核对同一目录、连接和维护状态，再 commit 工作连接事务。返回时沿用前端库作用域守卫，不将旧库结果展示到新库。
- Core prepare/commit 增加显式 `stop:&dyn Fn()->bool`，课程完整摘要复用 open_for_analysis(stop)，源附件分块复制及逐项导入前后检查停止，确认前再检查；Task12 可组合任务取消信号，Runtime 组合库/位置/维护变化。原 b1 测试通过薄 fixture helper 显式给 false，不跳过真实实现。
- `AttachmentSource.filename:Option<String>` 仅后台显示名，已登记附件用数据库原文件名，普通已批准来源用 path basename；扩展名/读路径仍由实际受控文件决定。名称不作为文件位置使用。
- 并发重复 prepare 在唯一身份冲突时复用已登记快照；并发 commit 的阶段性读失败先重读持久回执，不能再写正文或删除已提交附件。取消不伪装撤销已提交内容。
- Tauri 命令：`prepare_note_conversion_preview`、`get_note_conversion_preview`、`commit_note_preview`、`cancel_note_conversion_preview`。不新增操作系统文件打开或任意命令执行能力。

- [x] RED：真实 AppState 临时库中从注册附件准备/提交，显示名与图片字节保留；严格拒绝路径/多余字段/其他库 ID；切库/维护阻止写入；并发准备和重复提交仅一份正文/同一回执；受控停止不产生成功正文或遗留暂存文件。
- [x] GREEN：接通 UUID-only 命令、可取消内核和实际原子最终门，薄前端 API 路由保持 expectedLibraryId。
- [x] 定向/完整 Rust/前端/构建/凭据扫描；阶段本地提交随本记录保存。完整 Rust 463 通过、2 既有忽略，前端 384 项通过，生产构建成功。证据 `docs/evidence/merge/task9-note-preview-runtime.md`；尚不宣称实际 AppHandle/GUI/双应用联调或 Task9 完成。

## 9.3b3 受控预览内容读取（BASE d943661）

假设：本步只补齐展示所需读取能力，不提前激活迁入 UI；原文归档和预览副本已由前两步登记。复用现有 dcc-media 协议、worker connection 和原件读取，不新增协议或通用文件服务。

**Files / Interfaces：**
- `note_preview.rs`：`preview_image_response(db,base,library,previewId,placeholderId,stop)` 返回现有 HTTP 图片响应，仅 ready 快照中属于该预览的图片；检查快照/位置/副本摘要，保留只读句柄、限定100MiB，返回 no-store/nosniff。不把占位ID当普通附件读取，不开放 PDF/文本/远程文件。
- 同文件 `read_preview_block(db,base,library,previewId,placeholderId)->String`：只读该快照 unsupported 清单对应原字节范围，重新校验归档全摘要与 UTF-8；不注册片段、不写正文。
- `note_preview_runtime.rs`：复用当前库绑定、维护/位置检查，图片响应与原文读取均在锁外 IO，返回前丢弃旧库响应。
- `lib.rs`：现有协议增加固定 `note-preview/<library UUID>/<preview UUID>/<placeholder UUID>` 路由；原文新增 `read_note_conversion_block` UUID 命令。前端 typed API 添加原文读取，预览图片地址待只读页面接入时生成，不能写回唯一正文。
- [x] RED：真实 AppState 临时库验证图片响应及中文显示名不影响路径，另预览占位ID/路径/其他库拒绝、篡改及取消拒绝、原文字节 CRLF/HTML 精确且不执行；读取不增加附件、笔记或片段。
- [x] GREEN：最小实现受控读取边界与路由，已有笔记图片协议保持不变。
- [x] 定向、完整回归和生产构建、证据、本地提交随阶段保存。完整 Rust 467 / 前端 385 通过、构建成功；证据 `docs/evidence/merge/task9-note-preview-content.md`。未转换历史列表/导出/计数及实际预览页面随后实施，不标 Task9 完成。

## 9.4a 原件版本浏览、导出与计数（BASE 18fc88b）

复用 schema27/28 登记，不再建历史表或改旧正文。浏览当前库所有原件版本；未完整转换计数包含未提交和已提交但仍 pending 的版本，文案明确“转换记录”不等同现存笔记。历史读取独立于旧预览批准，恢复后仍可核对原件。

**Files / Interfaces：**
- 新 `learning/note_archive_browser.rs` 与 tests：`list_archives(db,offset,limit)->ArchivePage {versions:Page<ArchiveVersion>,unconvertedCount,partialCount}`，每页1–50，稳定created_at/id排序；conversionState=unconverted/partial/converted，由持久提交回执推导，缺字段/损坏JSON不当完整成功。只返回来源身份、时间、摘要、大小和状态，不返回磁盘路径。
- `read_archive_view(db,base,id)->ArchiveView {archiveId,sha256,size,text:Option<String>}`，复用已登记UUID全字节校验，非法UTF-8不替换编码。原字节导出仍读取完整文件，不从textarea重建。
- 现有 `note_preview_runtime.rs` 加 `list/read/export_archive_in_library` 与三条对应 `list_learning_note_archives/read_learning_note_archive/export_learning_note_archive` 命令。只读操作锁外IO，返回前重新核对库；显式导出只接受用户选择的 .md/.bin 新文件，拒绝链接目录和库内写入，fsync+persist_noclobber在短最终库门发布，禁止覆盖已有文件。
- 新 `src/features/learning/NoteArchiveBrowser.tsx` 与 tests；现有知识库导航加懒加载“原文归档”。使用原生dialog焦点限制与Escape/外部点击关闭、主题tokens、既有LoadFeedback/重试，50项分页、按需读取、UTF8只读文本、无效编码提示及原字节导出。只读浏览不触发付费AI、正文保存或重新转换。
- [x] RED：合成真实库的完整/部分/未转换三种计数、稳定分页、无效UTF8/原件篡改、备份恢复后读取、原字节导出完全一致与禁止覆盖/库内写入/错库；UI按需加载、错误重试、迟到旧库结果丢弃、显示纯文本而非HTML、导出取消不写文件。
- [x] GREEN：实现边界、三条typed API与导航入口；不激活转换提交页面，不按来源名称合并。
- [x] 完整Rust 470 / 前端392通过、生产构建成功、凭据扫描与本地提交随阶段保存。证据 `docs/evidence/merge/task9-note-archive-browser.md`；真实源迁入/IME/物理DPI仍留主计划，不把此步当全部Task9完成。
