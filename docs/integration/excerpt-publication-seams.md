# 本地学习摘录发布接缝（只读核查）

供主计划Task11的11b2使用。复杂接缝由GPT-6.1 Sol high只读核查，主线程核对现有代码；不是全分支最终审查，也不扩大迁移、付费AI、发布或安装授权。

## 复用边界

- `knowledge::save_in` 可在外层事务中使用，但接受完整正文。发布层必须在事务内取当前笔记、验证expectedRevision，仅向确认的顶层节点边界splice摘录，保留其他字段，`force_checkpoint=true`；不接受前端拼接后的整篇正文。
- `knowledge_files::import` 可用外层Connection，但内部临时生成附件ID并先发布文件，现有调用者无法预先记录完整崩溃清单。摘录需要预分配附件UUID并登记已准备文件的最小内部入口，复用原MIME/尺寸/摘要和归属校验。不同笔记有独立附件登记。
- `reference_boards::import_paths/add_paths_internal` 会自行开事务并把单图失败降为warnings/skipped，不能串入原子摘录提交。只提取现有 `insert_stored_item` 等事务内最小插入能力；无截图必须拒绝板目标，不生成空板项。
- `images::import_image_to` 可在私人暂存目录准备原图/缩略图，最终发布仍需reject_links、真实根校验、fsync与noclobber。不能把词法safe_join当作真实目录边界。
- 项目目标新建摘录笔记时，应包含真实project reference节点，让save_in重建索引；只插knowledge_references会在下次保存时丢失。发布时复查项目未归档；项目和板当前没有整数revision，不能假称已有expectedRevision。仅已有笔记严格校验revision，板/项目只做新增，不改旧条目。
- `note_preview` 只能借用持久回执及异常后重读回执的范式，不能调用Replace路径追加正文，也不能把其仅在内存的published列表当作崩溃恢复清单。

## v29持久化落点建议（实现前固定最终字段）

1. `learning_excerpts`：不可变preview/source快照，绑定library/root/course/media/file及真实SHA、字幕SHA、完整文字/ranges/comment、可选截图/SHA，payload及大小/版本/摘要/状态。
2. `learning_excerpt_operations`：operationId主键、excerptId唯一、确认请求及其摘要、pending/committed/cancelled、恢复清单相对路径、结果UUID和持久回执。摘要覆盖preview、目标、插入边界、expectedRevision和图片策略；同ID改请求拒绝，同preview换ID不能再次发布。不对内容SHA全局去重，允许用户明确创建新摘录。
3. `learning_course_notes`：课程/笔记关系，复用现有实体，解绑不删除实体。

回执不能因目标后续被删除而级联消失，否则旧operationId重试会重建对象。重复请求重放原回执，另行报告目标后来不可用。Committed与receipt必须在同一最终事务写入。

## 文件、事务和失败恢复

- 大IO在锁外：媒体完整SHA/只读lease、字幕/暂存图校验、空间检查、图片原件/缩略图准备及fsync。确认请求中的截图失败必须显式处理，不能悄悄当完整成功。
- 最终路径发布前，写入并同步操作专属恢复清单，记录本次预分配UUID、暂存/最终相对路径、大小和SHA。新笔记ID由save_in生成时，可在最终事务内建立未提交空笔记，取得ID后补清单；不能先单独提交空笔记。
- 短事务内完成目标复查、正文splice/save_in、附件或板项登记、课程/项目关系、operation receipt。文件系统没有SQLite原子性，采用“完整文件在先、DB可见在后＋持久清单”。
- commit异常/丢响应先按operationId读回执；DB不可读不等于未提交，不立即删文件。只有确认未提交，才清理本操作清单所列、SHA仍一致、当前DB未引用的新文件。不能删除既有附件、整目录或用户对象。崩溃后pending显示可重试；重试仍重新校验来源、库和目标修订，不自动续写正文。

## 锁顺序

11b1的取消轮询使用source_scope_changed.try_lock，WouldBlock只延后判断，不当作确认。摘录准备释放媒体lease后才进入最终强作用域核验；分析持lease的最终核验锁忙则明确重试，不放宽付费请求前检查。

11b2发布推荐：完整SHA及lease → active.try_lock → worker busy_timeout(0) → BEGIN IMMEDIATE → 短校验/文件发布/SQL/commit。active忙或SQLite BUSY立即退出，释放全部资源后再提示重试；不在锁内sleep，不沿用默认5秒SQLite等待。持active guard时直接读guard，不再调用active_library_id/with_library等重入方法。此顺序不复制旧note_preview的worker写锁→阻塞等待active模式。

## 旧备份恢复

在backup.rs两处既有invalidation链加入摘录：未提交preview/operation改cancelled，恢复后重新准备，即使library/root相同也不复用旧确认。保留已提交回执、笔记、附件、板项和关系；恢复不执行摘录撤回，不根据旧日志删除用户对象。只清理确认归属cancelled preview的私人暂存；最终路径/摘要变化/归属不明则保留并报告。现有整库备份包含learning-data、笔记附件和板快照；旧备份迁移新增空v29表。
