# DCC 学习摘录 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans inline. 按主计划Task11逐步实施，各步先测试后实现。

**Goal:** 从当前课程的真实时间段、完整字幕和可选单帧生成摘录，经明确确认后追加到当前库笔记、项目资料或参考板，支持去重和失败恢复。

**Architecture:** 复用 subtitles::verified_cues、LearningReference、media_access只读lease、knowledge::save_in 和已有图片快照存储；只新增本地摘录操作，不建立双应用交接或另一套笔记。父计划的v28已被笔记转换预览占用；需要存储时使用下一版本v29，不覆盖现有迁移。

**Tech Stack:** 当前 Rust/rusqlite/serde 与 React/Tiptap，无新依赖。

## 11a 可信文字草稿（BASE 0bfdd99）

**Files:** 新 `src-tauri/src/learning/excerpts.rs` / `excerpt_tests.rs`，仅在既有mod.rs注册模块及测试。无命令、schema或生产入口激活。

**Interfaces:**
- `ExcerptSelection {subtitle_id:Option<String>,cue_ids:Vec<String>,ranges:Vec<TimeRange>,comment:String,frame_time_ms:Option<u64>}`，camelCase/deny_unknown_fields；不接受客户端字幕文本或磁盘路径。
- `ExcerptText {course_id,media_id,title,subtitle_id,subtitle_sha256,ranges,cues,comment,frame_time_ms}`，保留完整选中段和原文；这是未截图/未发布的内部草稿，不是已保存摘录。
- `prepare_text(db,base,course_id,selection)->Result<ExcerptText>`：当前课程可用/已知有效时长；最多256个唯一cue/range，备注20,000字节、选中字幕1MiB上限，超限拒绝不截断。
- 有字幕选择时，使用同库课程的 verified_cues 重新验证原件hash并解析，按原件顺序取ID，未知/重复/跨课程ID拒绝。重叠或相接范围合并，间隙保留；若前端同时给ranges，必须与推导结果完全一致，不把不连续来源扩大为包围范围。无cue时使用用户明确范围，要求有序、不重叠、start<end<=duration。截图时间必须在范围内且小于时长，不在本步执行截图。
- `document(text,verified_media_sha256)->Result<Value>`：使用既有LearningReference::validate与唯一JSON模型，完整字幕及备注只变为text/hardBreak，不执行HTML。摘要必须由后续受控媒体读取取得，前端不得提供；本步不证明实际媒体未变化，也不开放IPC。
- 分配正文节点前检查换行总数，超过4096处明确拒绝并保留原文字草稿；仅限制原文字节数不足以阻止百万级hardBreak节点膨胀。此限制只用于新摘录，不限制已有笔记或历史导入。

- [x] RED→GREEN：非连续/重叠/乱序选择的原文完整性；篡改字幕与缓存伪造；跨课程/未知/重复ID；范围/时长/截图时间/超限/控制字符；JSON文本惰性与现有knowledge保存/检索/媒体反链兼容；草稿准备不写DB或源文件。
- [x] 定向7项通过；完整Rust480通过、2既有忽略，生产前端构建成功，前端代码未改，不重复K3的424项前端回归。证据 `docs/evidence/merge/task11-excerpt-text.md`；凭据扫描与独立本地提交随阶段保存。主Task11仍待截图、持久事务和UI接入。

## 后续顺序（未激活）

1. 11b持久preview/operationId及短事务追加：读取实际media_access/FFmpeg与save_in发布路径后冻结接口；媒体lease+完整SHA、字幕SHA与根/库身份复查，截图失败保留文字草稿且警告；目标修订冲突不覆盖，丢回执先读持久结果再清理自己的暂存。schema使用v29。
2. 11c学习区字幕选择/当前帧→预览确认UI、可靠重试和目标选择，原生遮挡复用现有overlay；无图不得创建空参考板图片。
3. 11d项目学习资料查询/精准来源跳转/解绑不删实体。新项目关系不能只写knowledge_references，否则save_in重建会丢；应复用正文真实project reference或明确关联表。
4. 每步测试与本地提交，最后Task14真实原生/长文/IME/DPI及完整来源回看验证。当前不触发真实用户迁移、付费AI、推送、合并、发布或覆盖安装。

## 11b1 媒体来源与单帧准备（BASE 84149c9）

**Files:** 新 learning/excerpt_runtime.rs / excerpt_runtime_tests.rs；现有analysis.rs仅复用来源读取边界、将原取帧循环调用提取到共享 media_engine/frame.rs；两处mod注册。暂不新增schema或Tauri入口，持久operation/提交在11b2。

- `prepare_in_library(state,expected_library_id,course_id,selection,resources,cancelled)->PreparedExcerpt`，仅Rust后端可调用。结构保存library/root身份、source_file_id/完整SHA/duration、11a文字、唯一JSON正文、可选单帧及warnings；没有任意源路径输入或已保存回执。
- 直接复用 analysis::live_source_controlled，只提升必要字段与该函数在learning内的可见性，不复制原件/批准/字幕校验，不为本步重构整套分析模块。抓帧期间持有现有CheckedMediaFile只读租约。
- `media_engine::frame::sample_jpeg(ffmpeg,&CheckedMediaFile,time_ms,stop)->dataURL` 由现有分析与摘录共同调用：固定安全参数、-nostdin/-n、单帧、1024像素、30秒超时、私人临时目录；复用现有有内存限制的JPEG转换。工具仍由现有runtime定位，临时文件自动回收。
- 取消、维护、错库、原件摘要变化均失败，不能变成普通截图警告。普通解码/截图失败保留已校验文字、frame=None并显式warnings；不返回“已保存”。无截图请求不调用FFmpeg。
- 返回前复查字幕SHA、课程/原件登记及根身份；释放媒体租约后才进入短AppState作用域锁，不持租约等待提交锁。后续11b2提交必须重新完整核验媒体SHA，不能把这份未持久化准备对象当可复用授权。
- 实际RED复现旧来源入口会接受其他媒体拥有的primary_file_id；在同一共享查询要求文件entry_id归属当前视频且role=main，同时修复分析与摘录，不在各调用方堆补丁。
- Windows真实只读lease＋有界写线程RED复现“库锁等待文件替换、文件读取取消轮询又等库锁”的环。共用source_scope_changed.try_lock用于分析/摘录读取与任务取消轮询；WouldBlock仅延后判断，不能作为确认。分析最终来源确认try_lock忙时明确重试，不绕过付费请求前核验；摘录释放lease后进入最终强校验。
- RED→GREEN：真实FFmpeg合成视频单帧/摘要/原件不变、坏视频保留文字而无图、没有截图时只读文字/实际媒体SHA、错库/取消/维护/源变化拒绝。回归原分析真实帧与其任务测试；完整Rust、前端构建、扫描及独立本地提交。
- [x] 定向摘录15项、课程分析32项通过；最终完整Rust488通过/2既有忽略，生产前端构建成功。证据 `docs/evidence/merge/task11-excerpt-runtime.md`。本步仍只有后台准备，不含持久保存或界面入口。
- 11b2发布接缝事实见 `docs/integration/excerpt-publication-seams.md`；高难度只读核查使用GPT-6.1 Sol high，未使用Astra，最终全分支审查仍Task14。

## 11b2a 不可变预览存储与恢复边界（BASE 650a344）

**Files:** 新 learning/excerpt_store.rs / excerpt_store_tests.rs / schema-v29.sql；mod迁移、backup.rs两处失效链；PreparedExcerpt/ExcerptText补严格反序列化与Clone；提取共享source_file_id查询供来源准备、最终检查及存储元数据复查。无前端命令激活。

- v29新增learning_excerpts（library/root/course/media/file、完整不可变payload/SHA、ready/cancelled/committed）、learning_excerpt_operations（operationId主键、excerptId唯一、请求/SHA、pending/committed/cancelled、manifest及receipt）和learning_course_notes关系。操作回执不对笔记/板做级联删除；committed必须有receipt，其他状态不能冒充回执。
- `save_prepared(db,base,library,&PreparedExcerpt)->StoredExcerpt{id,status,prepared}`；guarded版本提供最终事务提交回调，序列化在事务前。当前只有同内容ready预览可复用，取消/已提交后允许用户新建同内容摘录，不全局锁死内容SHA。最多16MiBJSON；元数据、正文重建、图片类型/解码大小/SHA和图文状态一致性校验。
- `get_preview(db,base,current_library,id)` 读取前先限字节及校验payloadSHA；ready严格绑定本库和原root。cancelled/committed仅作为当前库备份内的只读历史保留旧来源绑定，不作为可续写授权；始终校验调用者当前library marker，不能读另一个活动库。
- `cancel_preview` 幂等取消ready，但存在pending发布记录时拒绝直接取消，避免绕过恢复清单；已提交不删除实体。后续发布重试以operationId/确认请求摘要/回执为准，不调用转换Replace路径。
- `invalidate_previews` 只取消未提交预览及操作，加入普通恢复与中断恢复两处链；不删笔记、关系或历史回执。旧v28数据迁移保持正文和已有素材；现有8个schema版本断言仅升级到29，保留其他数据断言。
- RED→GREEN：持久重读/ready去重/跨连接并发、取消后可重建、错库/root/正文或图片篡改拒绝、payload摘要损坏、v28迁移/操作状态约束/回执不级联、真实整库备份恢复作废旧确认而保留笔记。完整Rust/构建/扫描及独立本地提交。

此步不生成“已保存摘录”回执、不发布目标附件；11b2b实现带清单的笔记/项目/板原子发布后再接UI。

- [x] 初始5项行为RED后实现；存储6项、全部摘录21项通过。完整Rust494通过/2既有忽略，生产前端构建成功，源码扫描476文件/0命中。证据 `docs/evidence/merge/task11-excerpt-store.md`。无前端改动，不重复上阶段424项界面回归。

## 11b2b1 文字追加、项目资料与可靠回执（BASE 16e73e0）

**Files:** 新 learning/excerpt_publish.rs / excerpt_publish_tests.rs，mod注册；subtitles.rs复用原件解析并提供保留只读文件句柄的内部入口。

- 先完成纯数据库文字提交，再扩展11b2b2截图/参考板文件清单；两步均无Tauri/UI入口。请求显式includeFrame=false；true在本子步明确拒绝，不能静默丢图。
- `CommitRequest {operationId,excerptId,destination,includeFrame}`；destination为New(title,notebookId)、Append(noteId,expectedRevision,insertIndex)或Project(projectId,title,notebookId)。仅接收UUID/确认边界，不接收正文或文件路径。
- 同operationId与请求摘要不符拒绝；同preview换operationId拒绝。先持久pending意图，再重新验证实际媒体完整SHA并持原只读lease、字幕原件hash和只读句柄。最终active.try_lock与worker busy_timeout(0)→IMMEDIATE短事务复核同库/root/source登记、项目未归档、目标当前revision，再save_in/课程关系/项目媒体关系/receipt原子提交。SQLite/active繁忙明确重试，不能带媒体lease等待锁。
- 追加只向确认的顶层insertIndex插入，保留原笔记字段、导航颜色和其他节点；force_checkpoint保存历史。项目新笔记包含真实project reference，索引随以后保存保留。
- 已提交操作先重放原始receipt，不因源后来变化或目标后来删除而再次创建；另返targetAvailable提示现状。最终提交异常先读持久回执，数据库不可读不推断未提交。pending失败可用相同请求手动重试；取消pending同时取消preview，不删除用户实体。
- RED→GREEN：新建/重复/丢回执、同ID改请求和换ID重复、追加位置/修订冲突与旧字段保留、项目关系/归档拒绝、来源变化/取消/错库、最终事务回滚与pending重试、并发同ID/关闭后重读。完整Rust/生产构建/扫描及独立本地提交。

Ruling：纯文字发布没有新文件，因此本子步不制造空文件清单；图片发布将在11b2b2加入预分配UUID与fsync清单后开放。若错在拆分顺序，代价是内部入口接通延后，不损失已保存内容。

- [x] 6项行为RED→GREEN，补并发/忙锁及Windows媒体和字幕只读句柄保护；全部摘录28项通过。完整Rust501通过/2既有忽略，生产前端构建成功。证据 `docs/evidence/merge/task11-excerpt-publish-text.md`。仍未激活IPC/UI或截图发布。

## 11b2b2a 截图文件清单与受控发布（BASE 31a3dba）

**Files:** 新 learning/excerpt_files.rs / excerpt_file_tests.rs及mod注册。先把共享文件边界完成，随后11b2b2b接入附件、板及回执，避免未经测试的文件操作直接进入笔记事务。

- `prepare_files(base,operationId,libraryId,rootHash,frame)->FrameFiles`：只接受后端PreparedExcerpt中的已验证JPEG（再次验证摘要、大小、尺寸），输出JPEG及WebP缩略图、尺寸、真实SHA和大小。复用images::import_image_to在私人临时目录处理，再原子复制到learning-data/excerpts/{operationId}/staging；保留既有相同内容，拒绝不一致和路径链接。源视频不复制。
- `PublicationManifest`绑定operation/excerpt/library/root/requestSHA、预分配图片UUID、可选预分配新笔记UUID及所有stage/final相对路径/大小/SHA。最终地址只允许严格构造的note-attachments/{noteId}/{imageId}.jpg，或reference-boards/{boardId}/originals/{imageId}.jpg及thumbnails/{imageId}.webp；不接受前端路径。
- 清单先fsync并以内容SHA命名，Locator使用learning-data/excerpts/{operationId}/{sha}.json，严格读取路径/大小/摘要/字段/目标绑定。清单初次创建前拒绝已存在的最终路径，不覆盖；已有同一清单允许复核后重试。
- `publish_files`：先完整核验所有stage和已存在final，再复制缺失文件；noclobber、fsync，返回只读校验后的全部结果。最后一个文件失败不能报告成功，留下清单可恢复；不在此模块决定SQLite提交或删除用户内容。
- `cleanup_uncommitted(db,base,manifest)`：调用者必须已确认操作未提交且位于原库/根；若DB不可读/manifest不匹配/任一目标已登记或内容SHA变了，则保留并明确报告。仅删除清单中的、当前仍匹配且未登记的新文件，不删目录、不删旧附件、源视频或用户实体；stage保留用于重试，提交后清理由后续集成调用。
- RED→GREEN：真实JPEG/WebP准备和完整摘要、清单篡改/越界/root拒绝、预先存在目标不覆盖、部分发布后重试、清理只针对未登记新文件且保留旧文件/篡改内容/源件。仍无IPC/UI。

Ruling：文件边界先单独证明，正式发布始终由11b2b2b持媒体lease和短事务完成；11b2b2a的文件复制本身不等于已保存摘录。

- [x] 4项行为RED→GREEN，补独立暂存篡改、重复准备和实际复制库根目录检查；最终全部摘录33项通过。最终完整Rust506通过/2既有忽略（82.93秒），前端生产构建成功。证据 `docs/evidence/merge/task11-excerpt-files.md`。

## 11b2b2b 截图与目标事务、回执接通（BASE de57a21）

**Files:** excerpt_publish/files及其测试；knowledge.rs最小预分配新笔记ID入口；knowledge_files.rs已发布图片登记入口；reference_boards.rs单图事务内入口；schema-v29.sql补板项来源关系。

- destination增加Board(boardId,x,y)，坐标有限且绝对值不超过1,000,000，要求includeFrame=true且准备结果真实有图；不生成空板项。笔记/项目允许显式仅文字，沿用已有确认请求。
- noteId/revision在receipt改为可选，新增可选boardId/boardItemId，空字段不序列化；旧文字回执保持可读。targetAvailable只指目标实体当前是否存在，不能因目标删除而重建。
- 图片处理/fsync清单在锁外完成，pending操作通过短作用域/IMMEDIATE事务只绑定一个manifest_relative。已有清单重试复用图片ID/新笔记预分配ID及全部文件SHA；同ID改请求继续拒绝。少量竞争中未绑定的自有清单保留，不删除归属不明材料。
- 新笔记UUID在清单阶段预分配；knowledge内部入口只允许新建、规范UUID且当前不存在，不能借保留ID覆盖旧笔记。必要的无图片初始笔记只在同一未提交事务内存在，附件登记后保存完整正文，清除仅该新笔记的中间历史；不提交空笔记。
- 截图带普通image节点和指向截图实际毫秒的LearningReference，仍保留完整原文及所有选择范围。复用现有附件协议，不提交磁盘路径给WebView。参考板复用stored image插入/触碰时间/z-index，learning_excerpt_board_items保留准确来源；关系级联只删关系，committed操作回执不级联。
- 最终active.try_lock→busy0/IMMEDIATE复查目标与来源，再publish_files、登记附件/板项、save_in/关系/receipt同事务完成；最终文件也获取FILE_SHARE_READ句柄保留到commit，防止已校验副本在提交窗口丢失。
- 任一SQL/复制/取消/提交异常先核对持久回执。已提交保留全部文件；数据库不可读保留并报待核对；确认pending后只清理清单中未登记且未改变的新文件。取消pending同样清理自有新文件，stage与清单保留供诊断，不动用户对象。恢复备份不自动按旧日志删文件。
- RED→GREEN：新笔记/追加/项目截图与协议读取、板项和准确来源、无图拒绝、旧修订不写附件、最终SQL失败清理/稳定ID重试、实际最终副本只读保护、丢回执不重复、真实整库备份保留已保存图片并作废pending。完整Rust/前端构建/扫描/本地提交后再接11c界面。

- [x] 初始截图集成4项RED后实现，补追加/项目/无图和旧回执兼容；摘录全部40项通过。额外控制连接代际测试RED复现同库标识不变时旧worker误报成功，修复为活动连接确认复查与作用域内重新打开写连接。该测试是受控模型，真实整库备份恢复另有集成测试，不等于真实桌面并发恢复验收。
- [x] 最终完整Rust513通过/2既有忽略（82.25秒），前端生产构建成功，证据 `docs/evidence/merge/task11-excerpt-image-publication.md`。未激活IPC/UI；主Task11后续11c/11d仍待执行。

## 11c1 统一后台任务与 UUID 读取边界（BASE 6d54a0b，待实现）

事实 `docs/integration/excerpt-ui-seams.md`。普通前端只读核查GPT-6 Sol medium；主实现仍inline，最后整体审查Task14使用高难度模型且不使用Astra。

- 新 `ExcerptTaskRequest` action=prepare(courseId,selection)/commit(CommitRequest)，接入现有TaskRequest::CourseExcerpt、统一串行任务/取消/手动重试和中断恢复，不建摘录队列或付费流程。结果仅previewId或可靠receipt，不向tasks.json复制截图/全文。
- 复用准备与发布函数；prepare结果在当前作用域内持久保存，commit沿用原operationId。纯输入预算/ID检查在入队前，提取现有selection验证复用，不分叉术语或课程规则。相同活动请求复用任务ID。
- 统一should_stop读取停止位避免克隆大payload；保留其他任务原停止语义。commit返回可靠成功回执后才来的停止显示已完成并提示，不能伪称保存已撤回。prepare停止只取消未被新操作认领的ready预览，不删除已发布内容。
- 提交前再检查取消（包括图片复制/SQL已完成但尚未commit的窗口），取消则回滚与按清单清理；提交后收到取消以持久回执为准。新增可复现测试先RED。
- UUID-only get_preview/get_operation及当前课程分页operation摘要，均检查当前库与根；历史只读，ready/pending不能跨库/恢复复用。请求/摘要损坏以逐条错误保留，不执行宽泛恢复或自动重写正文。
- Tauri入口spawn_blocking复用现有模式；读操作不进入任务队列，prepare/commit通过现有start_library_task。前端类型/API补齐，无视图激活。记录实际命令/计数，完整回归/构建/扫描和独立本地提交后继续11c2 Composer。

### 11c1a 先接统一任务（BASE6d54a0b，待实现）

新增excerpt_runner.rs/tests；TaskRequest::CourseExcerpt action=prepare/commit，结果仅UUID/receipt；选择预算提取复用。相同活动请求合并，TaskRegistry.should_stop直接查停止位；已经返回提交回执的任务不被迟到停止改成未保存。源准备结果在当前短作用域写preview；异常/停止不触碰用户实体。11c1b再加UUID读接口与前端类型。

测试先证明准备/保存缺口、取消/错库、同请求任务去重及重启中断（不自动继续）、迟到取消不否定可靠成功；补“图片已复制但未commit”的取消窗口RED并修复。任务runner测试使用隔离真实AppState/TaskRegistry，不把纯队列模型当原生UI验收。

- [x] 初始46项中41通过/5个新增缺口失败，随后全部46项通过26.26秒；完整Rust519通过/2既有忽略82.98秒，前端生产构建成功。证据 `docs/evidence/merge/task11-excerpt-tasks.md`。本步通过原有start_library_task支持CourseExcerpt；尚未增加前端按钮或独立UUID读命令。

### 11c1b UUID 预览/回执读取与前端线型（BASE037d8fb）

新excerpt_access.rs及tests，统一spawn_blocking命令：get_learning_excerpt、get_learning_excerpt_operation、list_learning_excerpt_operations（courseId/offset/limit≤50）、cancel_learning_excerpt_preview、cancel_learning_excerpt_operation、get_learning_excerpt_targets。所有请求只接当前库/实体UUID及分页，无路径/正文。目标选项复用现有projects(false)/reference_boards只读查询，不能使用会立即加素材的picker。

- 所有读在同一当前活动库作用域内，marker与expectedLibraryId一致才执行；ready/root检查复用store，已提交receipt不依赖原件仍存在。operation读取严格验证冻结请求/SHA/身份/回执；来源preview损坏可保留已提交回执并给warning，不从诊断记录重新写正文。
- 列表只查当前course UUID对应的本库操作，最多50条，不加载所有截图/正文；损坏记录保留为unreadable条目及错误，不拖垮其他行。为course查询增加新摘录表索引。未知course可返回空历史，错库不能降级查询当前库。
- cancel操作按UUID在服务端读取已冻结请求后调用原取消逻辑；cancel ready仍禁止绕过pending操作。取消不能删除已保存实体。读操作不进入任务队列；准备/确认仍走共享CourseExcerpt任务。
- 前端扩展共享LibraryTaskRequest和学习types/API；任务提交显式重建允许字段，丢弃运行时附带的createdAt/status/document/path等额外属性，避免旧笔记入参问题再现。字幕范围与LearningReference共享类型。暂不渲染新界面。
- RED→GREEN：当前库/错误UUID/根身份、分页和课程隔离、pending取消和已提交回执、来源损坏与单条操作损坏保留、目标查询无副作用；前端invoke形状/字段剔除/期望库传递。运行定向、完整Rust/前端、构建、扫描并本地提交后进入11c2。
`n- [x] 11c1b：摘录定向51项、前端接口3项通过；完整Rust524通过/2既有忽略，完整前端427通过，构建4.96秒成功。证据 docs/evidence/merge/task11-excerpt-access.md。

### 11c2a 摘录确认组件（BASE1760bd3）

先完成独立组件和行为测试，再由11c2b接学习区选择、当前帧与App精确导航。新增ExcerptComposer.tsx及测试，复用modal-card/backdrop、LoadFeedback、useRequestGate与统一任务API；无依赖或新队列。简单弹窗事实核查GPT-6 Luna medium，主实现inline，不用Astra。

- 外部输入只含当前库/课程、选中cue的ID/字幕ID或明确毫秒范围、可选当前帧和备注；后端生成完整原文/截图。只有用户点击预览才prepare，确认前绝不写笔记/板。
- 显示后端全字幕、分离时间段、备注和真实截图及警告，默认新笔记，可选项目/已有笔记/板。笔记读取当前revision与顶层插入位置；不把前端正文传到提交。参考板无图禁用。
- 提交第一次生成operationId后冻结所有目标/预览参数，失败/丢回执允许同请求手动重试或读取持久回执。取消确认操作走后端UUID取消；不通过改参数绕过去重。关闭只收起，后台任务继续。
- 当前课程分页操作历史可读取/取消pending、查看历史回执；恢复坏preview仍保留receipt，已取消不恢复写入。读取失败显式重试，不清空已有成功结果。显示未提交准备任务的预览入口，避免关窗后丢草稿。
- 请求门与组件库/课key隔离过期异步响应；modal阻断native、Tab循环/初始焦点/关闭回焦、Esc及外部点击遵守IME。使用已有主题token与有界滚动，原生/DPI人工验收仍Task14。
- 行为RED->GREEN验证明确确认、同operation重试、历史回执、错库结果拒绝、无图板禁用及键盘关闭；之后完整前端/构建。后端未改不重复Rust全量，沿用本轮524已验证结果。
`n- [x] 11c2a：新增8个前端行为用例通过；完整前端435项通过，生产构建成功。补恢复目标名称/冻结表单/毫秒时间边界均先失败再通过。证据 docs/evidence/merge/task11-excerpt-composer.md；播放器入口与导航待11c2b。

### 11c2b 学习区入口、显式范围与保存跳转（BASE4b33676）

在 LearningWorkspace 复用字幕多选回调和播放器快照打开 Composer；所选字幕只传ID，当前时间仅在选中范围内才提供截图。无字幕时可输入明确起止毫秒，当前有效帧默认形成一个1ms范围；未知/不合法时仍可打开记录但准备前需给出实际有效范围。切课程清空旧选择，复制时冻结打开瞬间来源，不追踪之后播放位置。

- 保存后打开笔记或板之前使用已有 flushWorkspaceEdits 退出守卫；失败留在学习区并报告，不假装跳转。App通过现有setView和目标ID交接到知识库/参考板，不新建导航协议。字幕、记录和来源本身不自动写入；无需付费AI。
- 先写行为测试：完整字幕ID/时间选择、跨课程不沿用、无字幕手动范围、无效截图范围、原生overlay占位和退出失败不跳转。mock测试不声称真实mpv验证；截图场景仍需Task14手工验收。
- 完整前端/构建通过后记证据并本地独立提交；11d再实现项目学习资料和参考板精确回源。
`n- [x] 11c2b：初始4项接入/手动范围RED→GREEN；补安全退出失败提示、切课隔离、原生启动未完成时退出、准确笔记/板ID导航、失败原因与最近任务顺序。定向43项、完整前端446项通过，构建4.28秒。证据 docs/evidence/merge/task11-excerpt-workspace.md。真实mpv/DPI/IME仍待Task14。

### 11d1 项目学习资料关系和受控读取（BASE c94c3be）

只读事实核查见本阶段ledger：不能仅依据同课程关联项目推断笔记属于哪个项目；knowledge_references由正文重建，不能把解绑实现为自动改写正文。新增 schema v30 的 project_learning_notes(project_id,note_id,created_at) 独立关系，允许项目显式关联当前库笔记；解绑只删除关联。迁移仅一次回填现有学习笔记里真实 project reference 所指项目，不按媒体反推；不改正文。之后再运行迁移不得复活已解绑关系。

- 项目摘录保存时，同事务写关系；笔记/项目永久删除由外键级联关系，笔记软删暂不展示且恢复后关联仍在。归档项目不允许新增关联，但允许查看/解绑，不删除笔记或媒体。
- Tauri list/link/unlink 严格要求当前library marker、规范UUID、项目与笔记真实存在；分页最多50，列表只返回标题、时间、课程计数，不复制正文或路径。只读接口与事务复用现有scope/SQLite语义，不访问映流数据库。
- 先测试v29→v30有且只有真实引用回填、两项目隔离、重复幂等、软删恢复、归档/跨库拒绝、解除与重新迁移后不复活；项目摘录发布/回执和回滚保持原语义。备份恢复证明仅关系变化且不破坏正文。全Rust、前端构建、凭据扫描后独立提交，再接项目UI和参考板回源。
`n- [x] 11d1：3项行为RED→GREEN，补提交回滚/重试和隔离库真实备份恢复，最终5项定向通过；完整Rust529通过/2既有忽略，生产构建4.25秒；证据 docs/evidence/merge/task11-project-notes.md。用户真实库未迁移。

### 11d2 项目学习资料界面（BASE42acbcc）

新增 ProjectLearningNotes 组件并在 ProjectHub 增加“学习资料”页；复用项目面板样式、笔记搜索、分页和当前库 UUID 命令。列表展示笔记及关联课程数，可打开实际 noteId、显式关联现有笔记或解除关系；归档项目不允许新关联，但可查看/解绑。关联和解绑只写关系，界面明确提示笔记与媒体保留。

加载、查询、提交按库与项目隔离并忽略旧响应；失败保留已读数据、显示可重试状态。App 使用 setView 的既有离开守卫进入知识库。先测命令范围与页面行为、延迟切项目隔离、归档限制和指定笔记跳转，再完成完整前端回归和构建。无后端修改，沿用上阶段529项Rust。
`n- [x] 11d2：项目资料页面与API线型6项RED→GREEN，入口/目标ID/解绑末页3项RED→GREEN；定向32、完整前端455项通过，构建4.29秒。证据 docs/evidence/merge/task11-project-notes-ui.md；参考板来源回看待11d3。
