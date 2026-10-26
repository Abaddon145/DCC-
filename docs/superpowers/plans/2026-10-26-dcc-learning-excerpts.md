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
