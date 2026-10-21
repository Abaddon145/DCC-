# 栈藏迁入映流学习能力 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. 用户已选择当前会话顺序实施；本计划待审阅，本轮仅编写计划，不授权立即集成、真实数据迁移、安装或发布。

**Goal:** 在栈藏内完成教程播放、课程分析、知识编辑、时间码摘录与项目回看，同时保留 v0.23 的素材、媒体、资产智能及所有用户数据。

**Architecture:** 栈藏是唯一主应用；复用当前媒体身份、项目、知识库、任务队列和凭据/额度。新增深层 learning module，调用小的后端 interface 处理课程、播放和分析；外部原件读取统一经 media_access interface，长期笔记只写 document_json，旧 Markdown 仅保真归档及显式导入导出。源端只按核实提交逐文件移植，不拼接第二套 App、数据库或任务队列。

**Tech Stack:** 当前 DCC Tauri 2、React 19、Rust、rusqlite 0.32、Windows x64、现有 Tiptap/Three.js/FFmpeg/yt-dlp；mpv 与可选 Whisper 在许可及完整工具校验通过后接入，依赖优先复用现有锁文件。

**Spec:** `C:/Users/14582/Documents/ChatGPT/视频下载软件/docs/superpowers/specs/2026-10-21-stackdcc-yingliu-merge-design.md`，本轮 SHA256 `558B07B7B6F00138F03E53A427F1EE402746DC075AAAB1B2E6D371E1423A11B6`。第 6.3 节已接受，不再次询问；正式执行仍须任务计划审阅。

## Global Constraints

- “以栈藏为唯一主应用，逐步迁入映流的原生播放器、课程字幕、学习进度、视频分析和时间码摘录能力。”
- “栈藏结构化正文成为合并后唯一默认编辑与保存来源。”
- “首版保留 Markdown 导入导出与原 Vault，不做 Obsidian 双向自动写回。”
- “原 Vault 文件不改动。”
- “栈藏已有凭据优先，不覆盖。”
- “迁移不自动解密或复制映流凭据，也不迁移 Cookie 临时明文。”
- “新增付费修复请求先确认，不无限自动重试。”
- “后台截图／写入重试仍须有持久操作 ID 防重复。”
- “同一视频可能属于多个系列或项目，不能因文件去重丢掉学习顺序和上下文。”
- “用户后续已编辑的对象，不因撤回迁移而被删除；出现冲突时停止并提供清单。”
- “原生播放、侧栏和编辑交互在真实 Windows 物理 DPI 100%／125%／150% 与跨屏环境通过。”
- “模拟服务、真实桌面、真实站点及付费 AI 验收分开报告；不未经授权调用付费 API、替换用户安装、合并 PR 或发布 Release。”
- 仅修改栈藏仓库；历史双应用协议/提交保留，停止配对、管道交接、外部回执服务。保留现有资产核心和智能层，版本号到交付阶段再定。

## Review Focus

1. 外部原件被同路径替换/重解析、恢复备份带回旧授权：禁止越权读取或删除原件；Task 2/3/13 检查。
2. mpv 晚到事件跨库、退出时保存失败、原生子窗口遮挡弹层：不写错库、不丢草稿、不泄漏进程；Task 4—6 检查。
3. 同文件多系列、进度分叉、旧路径型笔记身份：保留各上下文，不能按标题或最大时间覆盖；Task 4/7/12 检查。
4. Markdown 特殊块/外部修改、附件半发布、撤回前目标已编辑：不丢字节或删除后续内容；Task 9/11—13 检查。
5. 历史 AI 来源不完整、取消/重启、月度额度已用尽：只复用可核验阶段，不自动发付费请求；Task 8/12 检查。

---

## M0：实际基线、来源与未满足依赖

- 目标产品基线 `48078e56e0b304dc313a03c9aba4ef010847dacb`（0.23.0/schema 22）；计划起点当前 `acd1833`，分支 `codex/yingliu-learning-integration`。其上仅有基线测试/准备文档，不回退至历史 PR #14/v0.21。
- 本端依据：[readiness](../../integration/merge-readiness-dcc.md)、[基线](../../evidence/learning-upgrade/baseline.md)。同日已执行前端 315、Rust 331 通过/2 忽略及生产构建；这些不是移植后通过数。
- 源清单 `C:/Users/14582/Documents/ChatGPT/视频下载软件/docs/integration/merge-readiness-yingliu.md`，本轮 SHA256 `7C4ECEB13DFFAC9D91E26761CF0E94F7858D05BC132A8FAFCBB6403BEA243AD8`。源 HEAD 实测 `c02c63d`，完整产品起点 `67ba053`；分析最小修复尚未提交。源清单提供的能力、261/8 测试等是源端报告，不是 DCC 已复测结果。
- 源当前仍有 F11 小写错误，A–B/三态侧栏/完整字幕摘录等未完成；不把历史 8 键测试当完整键位验收。
- 源工作区缺 mpv/Whisper 完整工具，原工作树资源可用性需源端提供；源 mpv manifest 固定版本 `20261015-git-492bae0768`、7z SHA256 `f9becefe40e2a29d2ce5f3ab231ff094f9cf3e4038ff014cfe4a0c09ad90e79e`。在目标实际解压/校验和许可清单完成前，不能宣布可制作完整安装包。
- 源 Vault 身份由路径/卷/文件标识派生，笔记主键为 vault_id+path，**没有可直接复用的持久 noteId**。M6 创建确认的 source-profile UUID，保留原键和摘要映射。

### Task 1 / M0：冻结可移植输入与执行起点

**Files:** 更新 `docs/integration/merge-readiness-dcc.md`；新增 `docs/integration/yingliu-source-map.md`、`docs/evidence/merge/m0.md`，样本放忽略的 `release-artifacts/yingliu-merge/m0/`。

**Interfaces:** `yingliu-source-map.md` 固定源 SHA、逐文件源→目标、修复提交、许可证/工具 hash、样本 schemaVersion、转换版本；这是审阅清单，不是产品跨应用协议。

- [ ] 读取本计划/最新设计、两端 readiness，核对目标 HEAD/干净状态及源已提交修复；源端未提交差异只登记，禁止当作可移植提交。记录实际 SHA 后再改依赖它的目标文件。
- [ ] 核对 Task 2—7 的源/目标 interface；原生部分只选 `learning/native_player.rs`、`native_window.rs`、前端播放/绑定/事件算法及必要 `native.rs`。剥离 LearningService、源数据库/全局App/命令注册依赖，不整文件盲拷。
- [ ] 复核 Windows x64 mpv/FFmpeg/Whisper 构建、来源、资源名及许可；FFmpeg/yt-dlp/Deno 保留 DCC 已有实现，只有能力实测不足才换构建并单独审阅。源清单里的候选前端名字先 `rg --files` 确认。
- [ ] 跑 `npm test`、`npm run build`、`cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`。Expected：退出 0，逐项记录真实数字/既有忽略；沙箱 Temp 错误正常权限重跑并保留失败证据。
- [ ] 复用双隔离库恢复检查；登记可解码短视频、字幕/多音轨/中文字幕、BOM/CRLF/Obsidian 块和中断迁入样本。无用户库/私密字幕/密钥提交。Expected：原文件 hash 不变，库 UUID 和实体 UUID 不混用。
- [ ] 独立提交：`docs(merge): freeze target baseline and portable source manifest`。M0 门槛：设计/计划审阅，M1 文件/接口已确定；M2/M3/M6 特定缺口按下面各门槛等待，不能用猜测代替。

## 文件地图与增量存储决定

以下均为**拟新增**，不是当前已存在实现；M1—M2 按下列 interface 实现。M3—M6 的源特定转换器取得固定源格式后补子计划。

| 文件 | 职责 |
| --- | --- |
| 新 `src-tauri/src/media_access.rs` | 登记文件只读解析、外部源核验/重新定位、本机授权；所有外部视频消费走此 interface |
| 新 `src-tauri/src/learning/{mod,models,store,commands}.rs` | 本库课程/系列/进度及类型化命令，使用 AppState 的 Connection，不另开 learning.db |
| 新 `src-tauri/src/learning/{schema-v23,schema-v24,schema-v25}.sql` | 外部原件、课程/系列、字幕分阶段迁移；db::open_database 串行接入 |
| 新 `src-tauri/src/learning/{native_player,native_window}.rs` | 按来源清单适配 mpv 生命周期及 Win32 视口；仅进入学习时初始化 |
| 新 `src/features/learning/{LearningWorkspace,NativeLearningSurface,LearningControls,TranscriptPanel,LearningSeries}.tsx` | 学习工作区和按需面板；不复制源 App |
| 新 `src/features/learning/{api,types,nativeBindings,playerInteraction}.ts` | 目标命令、毫秒模型、键位与交互；复用 src/lib/workspace.ts 的 RequestGate |
| 现有 `src-tauri/src/{lib,db,state,tasks,media_library,backup}.rs` | 注册、维护屏障、任务、文件读取和备份接入 |
| 现有 `asset/{repository,service,metadata}.rs`、`db.rs` 清理入口 | 当前核心资产原件解析/处理、回收清理适配外部只读源 |
| 现有 `src/{App.tsx,types.ts,lib/api.ts,lib/personalization.ts}`、`MediaLibraryView.tsx`、`ProjectHub.tsx` | 内部导航及媒体“学习”，使用现有知识保存屏障 |

### schema 方案（执行时若 baseline 已变，重新分配号码）

**23 / Task 2**：`media_external_files(file_id PK FK media_files ON DELETE CASCADE,canonical_path,size_bytes,mtime_ms,sha256,volume_id,file_identity,revision,last_checked_at)`。仅登记 video 的 main 文件；core origin 仍为 media，media_files/asset_files UUID 不变。不重建 media_files：外部 main 的 rel_path 用保留相对定位符 `external-references/<fileUUID>`，它不是磁盘原件，存在 external 行才可解析；托管插入禁止使用该保留前缀。存储模式由是否有 external 行得出，DTO 明确 `storageMode: managed|external`，不伪称已复制。代理/缩略图始终托管。

**24 / Task 4**：`learning_courses(id PK,media_entry_id UNIQUE FK media_entries ON DELETE CASCADE,title_override,position_ms,duration_ms,rate,completed,progress_revision,created_at,updated_at)`；`learning_series(id PK,name,description,revision,created_at,updated_at)`；`learning_series_courses(series_id FK,course_id FK,position,PRIMARY KEY(series_id,course_id))`。position_ms 默认0，duration_ms可NULL表示未知，rate默认1，completed默认false，progress_revision默认0；CHECK 非负毫秒/有限合法速率由 Rust+SQL合理约束校验，排序索引 `(series_id,position,course_id)`；同视频一个课程、多系列成员；项目上下文复用 project_media_entries。

**25 / Task 7**：`learning_subtitles(id PK,course_id FK,relative_path UNIQUE,sha256,format,created_at)`；`learning_subtitle_cues(id PK,subtitle_id FK,ordinal,start_ms,end_ms,text,UNIQUE(subtitle_id,ordinal))`，唯一索引(course_id,sha256)，cue范围非负且end≥start，同摘要重导入复用cue ID。所有迁移事务、幂等，不修改已发布的旧迁移语义。

**后续阶段**：M3 26 学习分析/阶段缓存；M4 27 原文归档与笔记扩展；M5 28 摘录/笔记课程关联；M6 29 迁入批次/来源映射/撤回日志。号码为排期，不提前建空表。精确列见对应子计划，源清单未固定的列不在此猜成已有接口。

### 统一文件和备份边界（Task 3 起）

- `learning-data/subtitles/`：用户字幕/必要转录；`learning-data/analysis/`：可复用已完成结果及其必要出处；`learning-data/excerpts/`：已发布摘录来源；`learning-data/archives/`：Markdown/历史原字节与导入清单。均为本库托管相对路径。
- `.dcc/runtime/learning/`、`.dcc/runtime/imports/`：可恢复运行暂存，不能当持久原文归档。任务 journal 继续现有本机位置，不混入整库包。
- 外部视频只备份登记元数据，导出前/恢复后报告未归档原件数量；不把外部路径内容打包或递归删除。显式“归档复制”在 M6 完成后可纳入原件备份。
- 新增 export/restore/RESTORE_TARGETS/recovery/manifest allowlist/state::migration 同步支持 learning-data；所有子路径拒绝重解析/穿越，清理只作用本库自己发布的文件。备份不包含本机文件访问批准、Key、Cookie或原Vault。

## M1：目标接入与源端可靠性准备

### Task 2 / M1.1：外部视频只读登记和统一文件解析

**Files:** 新 `media_access.rs`、`learning/schema-v23.sql`、`learning/mod.rs`；改 `lib.rs`、`db.rs`、`media_library.rs`、`asset/repository.rs`、`asset/metadata.rs`、`models.rs`、`commands.rs`、`src/types.ts`；测试写在新 module 及既有媒体/资产测试。

**Interfaces（拟新增）:**
- `preview_external_video(state:&AppState, expected_library_id:&str, path:&Path, stop:&dyn Fn()->bool)->Result<ExternalVideoPreview,String>`：仅从明确文件选择/迁入清单调用。ExternalVideoPreview包含随机previewId、libraryId、name、sizeBytes、sha256、durationMs:Option<u64>和expiresAt；不返回任意文件读权限。
- `commit_external_video(state:&AppState, expected_library_id:&str, preview_id:&str)->Result<String,String>`：返回 media_entries.id；预览令牌绑定库/文件身份/摘要，15分钟失效，只能确认一次；重试有已提交结果。
- `resolve_media_file(db:&Connection,base:&Path,file_id:&str,grants:&LocalMediaGrants)->Result<CheckedMediaFile,String>`；CheckedMediaFile 是后端私有只读句柄/路径/摘要，不 Serialize 到前端。managed 用旧 safe_relative，external 用登记记录+本机批准；路径不能作为控制/播放命令参数。
- `relocate_external_video(state:&AppState,expected_library_id:&str,file_id:&str,preview_id:&str,expected_source_revision:i64)->Result<String,String>`：返回原media entry ID；显式确认新位置，不按同名扫描；相同内容保留身份，不同 hash 返回 SOURCE_CHANGED 并要求新源绑定。
- LocalMediaGrants 放 `%LOCALAPPDATA%/DCCAssetLibrary/learning-access/`，绑定库 UUID+实际库根路径、file UUID、真实路径/文件标识/hash；不随库备份恢复自动授权。

- [ ] 写失败测试 `external_registration_keeps_source_read_only` / `external_resolution_rejects_reparse_and_stale_grants` / `managed_resolution_is_unchanged`：原件 hash/mtime 不变；UNC、设备路径、ADS、目录、exe/脚本、symlink/junction拒绝；错库/过期预览不能提交。MP4/WebM/MOV/MKV/WMV/AVI 需扩展名白名单且 ffprobe 有视频流，不能仅看后缀。

  验收断言：`assert_eq!(source_after, source_before); assert_eq!(entry_id_again, entry_id); assert!(resolve_media_file(&db, &base, &file_id, &empty_grants).is_err());`。fixture先生成合法短视频；拒绝用空文件导致失败冒充访问控制有效。
- [ ] Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib media_access -- --test-threads=1`。Expected：新用例因功能缺失 FAIL；不是因工具缺失/导入语法错误。
- [ ] 实现 schema23、预览→确认、hash/FFprobe锁外执行及提交前复核；相同 checksum 复用现有非删除媒体，回收站命中提示恢复；不建第二份媒体/课程。针对已有登记源句柄防替换，重解析检查使用 Windows真实句柄路径/文件标识。
- [ ] 将 protocol_response、asset repository 原件读取/metadata/重试/health 的 mediaFile 分支导向统一解析；原 managed 分支不放宽。bundle_response 仍只允许托管3D依赖。全库搜索所有 media_files.rel_path 读取/删除 caller 并逐一记录适配结果。
- [ ] 测试软删除/永久删除/文件夹清理：只删登记和自有代理缩略图，外部原件始终存在；从任意 UUID 不能读取其他文件。恢复/复制库失去本机批准时显示“需重新确认”，不假成功。
- [ ] Run 定向及 Rust 全量（公共命令见末尾），Expected 全绿、schema22→23幂等、旧UUID/触发器/版本引用不变。Commit `feat(media): register external videos through checked read-only access`。

### Task 3 / M1.2：学习文件进入现有备份与恢复链

**Files:** 改 `backup.rs`（export/restore/RESTORE_TARGETS/recovery/allowlist）、`state.rs`（迁移）、`StorageSettings.tsx`；新 `learning/files.rs`；复用已验证备份测试。

**Interfaces:** `learning_file(base:&Path,relative:&str)->Result<PathBuf,String>` 仅解析 learning-data 的托管普通文件；`invalidate_external_grants(library_id:&str,root:&Path)->Result<(),String>` 在恢复/迁移完成后使原机批准失效；`external_media_backup_summary(db:&Connection)->Result<ExternalBackupSummary,String>`，该DTO字段count:u64/bytes:u64，只是登记摘要，不读取外部内容。恢复中若批准失效处理失败，外部访问默认拒绝，不能返回已获批准。

- [ ] 写 `learning_data_backup_restore_keeps_archives_not_external_originals`：两个隔离库、合法字幕/原字节Markdown/引用图进入清单；临时WAV、工具、grant、Key不进入；恢复后受控目录hash相等，外部原件未复制/删除，需重新确认路径。
- [ ] Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_data_backup -- --test-threads=1`。Expected：当前白名单未包含learning-data，FAIL。
- [ ] 同步导出/恢复/中断回滚/切库迁移所有目录列表及白名单；路径重解析拒绝而非静默漏文件。备份失败保留原库；版本不升备份包format，旧包无learning-data时恢复为空目录。
- [ ] 写丢清单/篡改hash/恢复中断/缺少外部原件/新库恢复用例及设置页“外部原件未归档”摘要，不宣称整视频已备份。Run 定向、`npm test` 和 Rust 全量，Expected全绿。Commit `feat(backup): preserve learning data and disclose external originals`。

### Task 4 / M1.3：课程身份与进度基础

**Files:** 新 `learning/{models,store,commands}.rs`、`schema-v24.sql`、`src/features/learning/{types,api}.ts`；改 `db.rs`、`lib.rs`。此时无播放器UI和独立学习导航。

**Interfaces:** `ensure_course(db:&Connection,media_id:&str)->Result<Course,String>` 验证当前非删除video，UNIQUE保证一个媒体一个课程；`course(db:&Connection,id:&str)->Result<Course,String>`；`set_series_members(db:&mut Connection,series_id:&str,expected_revision:i64,course_ids:&[String])->Result<Series,String>`（完整有序、不重复）。Course DTO 用整数毫秒和revision；Task5定义后端 PlaybackLease 才可写运行时进度。

Course字段：id/mediaId/title/positionMs:u64/durationMs:Option<u64>/rate:f64/completed:bool/progressRevision:i64；Series字段：id/name/description/revision/orderedCourseIds。标识均为UUID，课程名默认从当前媒体解析，titleOverride非空时采用用户值。
同时定义后续进度模型ProgressInput{positionMs:u64,durationMs:Option<u64>,rate:f64,completed:bool}、Progress=上述字段+revision:i64；持久更新必须是后端当前会话授权，Task5建立PlaybackLease后才开放运行时写入。

- [ ] 写 `learning_course_reuses_media_across_series`、`learning_store_rejects_foreign_media_and_order_conflicts`：同媒体两个系列仍一课程；同名不同媒体不混；乱序修订拒绝；项目关系和旧资产关系不被改；软删除不可播放、恢复重新可见。
- [ ] Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_store -- --test-threads=1`。Expected新能力FAIL。
- [ ] 实现schema24及串行库检查命令；使用现有 `with_library_if_id` / mutable 版本，错库显式错误，不回退默认库。
- [ ] Run 定向+Rust全量+`npm run build`，Expected全绿，备份恢复保留课程/系列/UUID。Commit `feat(learning): bind library courses and series to existing media`。

**M1门槛：** 上述已有服务适配/恢复检查通过；源端最小分析修复提交号和复现fixture固定后方可接入M3。此门槛不要求先完成远程AI调用或旧库迁入。

## M2：原生播放与学习数据（执行级任务）

### Task 5 / M2.1：mpv适配与生命周期

**Files:** 新 `learning/{native_player,native_window}.rs`、`scripts/prepare-learning-tools.mjs`、`scripts/qa/verify-dcc-learning-native.mjs`；改 `learning/{commands,models}.rs`、`state.rs`、`lib.rs`、`runtime/{tools,process}.rs`、`Cargo.toml` 的必要 Win32 feature、tauri资源配置；工具固定清单/许可落在 `src-tauri/resources/learning-components.json` 和现有 notices。当前DCC没有scripts目录，这些为计划新增。

**Consumes:** Task2 CheckedMediaFile、Task4 Course、AppState库检查/维护状态；源 `native_player.rs` 的会话/IPC/几何算法，`native_window.rs` Win32子窗口，来源SHA必须在Task1清单。

**Produces（拟新增）:** `LearningRuntime` 单个主窗口播放会话，延迟创建；所有命令通过AppState调用，均返回Result。`start_learning_playback(expected_library_id:String,course_id:String)->Result<PlaybackSnapshot,String>`、`control_learning_playback(expected_library_id:String,session_id:String,action:PlaybackAction)->Result<PlaybackSnapshot,String>`、`set_learning_viewport(expected_library_id:String,session_id:String,viewport:NativeViewport)->Result<(),String>`、`stop_learning_playback(expected_library_id:String,session_id:String)->Result<(),String>`。不接受path/任意mpv命令；seek用u64毫秒、rate 0.25—4、volume 0—100、轨道ID来自当前快照。`PlaybackSnapshot{sessionId,libraryId,courseId,ready,positionMs:u64,durationMs:Option<u64>,paused,rate,volume,muted,ended,tracks,error:Option<String>}`。`PlaybackLease` 后端私有绑定库/课程/会话epoch，阻止旧事件。NativeViewport复用源校验的逻辑像素字段，一次换算物理像素。

PlaybackAction使用serde tag枚举及deny_unknown_fields：Pause(bool)、Seek{positionMs:u64}、Rate(f64)、Volume(u8)、Mute(bool)、Subtitle/Audio{trackId:Option<i64>}、FrameStep{direction:forward|backward}、Aspect{ratio:source|16:9|4:3|fill}、SubtitleDelay{delayMs:i64，绝对值≤600000}、AbLoop{aMs:u64,bMs:u64}、ClearAbLoop、Fullscreen(bool)、Shortcuts(Vec<NativeBinding>)。Task7添加LoadSubtitle{subtitleId:UUID}，后端解析登记字幕。未知duration禁止设循环，不能用0伪装已知时长。
Task5一并提供 `save_learning_progress(lease:&PlaybackLease,seq:u64,progress:ProgressInput)->Result<Progress,String>` 给原生事件/退出flush使用；校验当前会话epoch及递增seq后持久更新。Task7复用并加入持续播放时的合并保存，不重新定义第二条进度写入通路。

- [ ] 写 `learning_native_session_is_lazy_scoped_and_owned`：普通启动/native-host分支不启动mpv；无效/回收媒体、错库、维护中拒绝；源件校验失败不创建进程；停止/重复停止/崩溃仅释放所属会话。写fake process测试明确不能替代真实mpv。
- [ ] Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_native -- --test-threads=1`。Expected缺能力FAIL。
- [ ] 固定mpv二进制与fonts.conf、实际可执行hash及来源许可证；mpv置现有resources/media-tools，字体配置保留相对目录。`node scripts/prepare-learning-tools.mjs --verify`校验固定manifest、真实exe/hash和资源，Expected退出0；工具缺失或tampered必须失败，不能直接给未知mpv放行。runtime::tools目前只对yt-dlp/deno验证hash，此步补mpv分支，不覆盖原工具。
- [ ] 进程启动等待/超时可取消；从现有runtime::process提取可共享的Windows Job创建/挂接/回收操作，提供长期 `OwnedProcess`，保留同步capture接口及其所有回归测试。由一个Job拥有播放器进程树，不能双重拥有源native::Job和目标Job，不能把会等退出的capture直接用作长期IPC播放器。
- [ ] 实现typed IPC、源句柄持有、UI主线程窗口创建/销毁、事件→ms转换和epoch检查。source LearningService 的DB/配置/知识写入依赖改为Task2/4的目标interface，绝不复制另一个state/SQLite。
- [ ] 写 `learning_native_exit_respects_note_save_and_library_maintenance`：退出先走现有笔记保存和任务屏障，不能从Destroyed递归调用exit；用户取消退出保留会话。切库先持久保存/停止，失败留原库；小窗/全屏只更新geometry。
- [ ] Run定向/Rust全量；新增 `scripts/qa/verify-dcc-learning-native.mjs` 以真实目标构建检查冷开/连点/切课/损坏视频/mpv丢失/退出后无孤儿，保存视频hash与会话计数。Expected退出0且native=true证据；工具未就绪则M2.1不通过。Commit `feat(learning): host scoped mpv sessions inside DCC`。

### Task 6 / M2.2：学习入口、控制及焦点

**Files:** 新 `LearningWorkspace.tsx`、`NativeLearningSurface.tsx`、`LearningControls.tsx`、`nativeBindings.ts`、`playerInteraction.ts` 及对应test；改 `App.tsx`、`MediaLibraryView.tsx`、`ProjectHub.tsx`、`src/lib/personalization.ts` / `types.ts` 的合法view保存列表。新增UI用现有样式/反馈组件。

**Interfaces:** `LearningWorkspace({libraryId,courseId,projectId?,onBack,notify})`；媒体详情“学习”先Task4 ensure_course再内部导航。学习是视频工作区view/继续学习快捷视图，不另加视频库。`nativeBindings(commands:ReadonlyArray<LearningBinding>,focus:'learning'|'editor'|'dialog')->NativeBinding[]`；LearningBinding字段id/shortcut/scope/enabled?，NativeBinding字段id/key，只有已注册的学习动作可绑定，editor/dialog返回空数组。`classifyVideoPointer(event,pendingClick)->PlaybackIntent` 只返回受控click/doubleClick/scroll意图，不返回任意command。

- [ ] 写 `native_bindings_preserve_all_function_keys`（F1—F12原大小写、修饰键、Plus、Shift数字），使用源默认整套而非8键子集；写single/double-click判定、双击无额外pause、滚轮仅视频区域改变5点音量、输入/IME/弹层优先。Run `npm test -- src/features/learning/nativeBindings.test.ts src/features/learning/playerInteraction.test.ts`，Expected源算法复现F11失败再修正。

  核心断言：`expect(nativeBindings([{id:'fullscreen',shortcut:'F11',scope:'learning'}], 'learning')[0].key).toBe('F11')`；双击的intent序列只有fullscreen，无pause；数字Shift及Plus用与Rust同一组golden值核对。
- [ ] 实现内部入口、返回项目/视频详情、RequestGate旧响应保护、正常加载/工具缺失/可重试错误。离开先知识保存屏障，失败停留保留草稿；重复进入同课程不建第二会话。
- [ ] 实现播放/暂停、精确seek、速率/音量、音轨/字幕选择、字幕延迟、逐帧、比例、全屏和右键菜单。新延迟/循环等PlaybackAction只列受控字段、数值范围，Rust验证；输入F11与原生焦点同一映射。A–B为 `0<=a<b<=durationMs`，设置后同一会话，切课清空。
- [ ] 加 `LearningWorkspace.test.tsx`：已有素材/项目/笔记准确返回、旧库事件不生效、工具缺失不空白、modal打开时native隐藏/避让，修订保存失败不离开。Run `npm test -- src/features/learning`，Expected全绿。
- [ ] Run全前端/Rust回归/构建，扩展真实QA脚本覆盖默认全键、100/125/150%DPI、多屏、最小980×680、全屏、参考板悬浮窗与截图弹层。Expected会话不重建、窗口无悬空、退出回收；未实测项不能勾完成。Commit `feat(learning): add focused playback controls and internal navigation`。

### Task 7 / M2.3：字幕、课程系列和进度持久化

**Files:** 新 `learning/subtitles.rs`、`learning/schema-v25.sql`、`TranscriptPanel.tsx`、`LearningSeries.tsx`；改store/models/commands/native_player、db迁移入口，使用Task4课程表；字幕文件置learning-data/subtitles。

**Interfaces:** `save_learning_progress(lease:&PlaybackLease,seq:u64,progress:ProgressInput)->Result<Progress,String>`，同会话seq递增，epoch新会话重置但不得接受旧会话；ProgressInput含positionMs:u64/durationMs:Option<u64>/rate:f64/completed:bool，Progress为这些字段加revision:i64，完成仅用户明确设置或真实EOF。`import_learning_subtitle(courseId,selected_path)->Result<SubtitleId,String>` 只接受明确选择SRT/VTT，复制/校验后登记；`transcript_page(courseId,subtitleId,offset,limit)->Result<Page<TranscriptCue>,String>` 限limit≤50，cue字段id/subtitleId/startMs/endMs/text。所有命令另带expected_library_id，内部只读/事务函数使用现有Connection，防重复返回原ID。

- [ ] 写 `learning_progress_ignores_late_epoch_and_sequence`、`learning_series_preserves_shared_course_order`、`learning_subtitle_keeps_full_unicode_ranges`：晚事件/重启/切库拒绝；重复导入不重复；中文字幕完整，不用slice截断选择；页外可明确加载/选择；超限提示而非静默截断。
- [ ] Run Rust `--lib learning_progress` / `--lib learning_subtitle` 和 `npm test -- src/features/learning/TranscriptPanel.test.tsx`，Expected缺能力FAIL。
- [ ] 事件持久化最多每2秒合并一次，pause/stop/切库/退出强制flush，错误可见不假报已保存；保存当前position而非max。系列排序事务/修订保护，跨系列关联不复制课程；字幕/章节按真实时间导航。
- [ ] 测试断电/进程重启恢复最近确认进度、同时多项目同课程、字幕删除/缺失、流时长未知时禁止伪造范围。字幕自动转录不在M2触发，留M3。
- [ ] Run定向、全回归、生产构建，真实视频中字幕/多音轨、A–B、EOF、继续学习、两次切课检查。Expected原视频hash不变、库恢复保留进度/系列、seek精确到播放器能力并记录误差。Commit `feat(learning): persist course progress series and subtitle cues`。

**M2门槛：** 已在DCC真实Windows进程播放，冷/热工作区入口、全默认键位/焦点/DPI/全屏与退出可用；浏览器模拟和源码存在不算通过。

## M3：统一课程分析任务

### Task 8 / M3：课程结果适配与预算

**Files:** 拟新增 `learning/{analysis,analysis_store,analysis_tests}.rs`、`learning/schema-v26.sql`；改 `tasks.rs`、`translation_service.rs`、`TaskCenter.tsx`；新学习分析页签。不得覆写asset_ai_analysis或搬源ai App/队列。

**依赖/Interfaces:** 等源端提交分析修复/fixture后，在 `yingliu-source-map.md` 固定结果schema、sectionValidation、模板/提示版本和转录/画面ID格式，再补 `docs/superpowers/plans/2026-10-21-dcc-course-analysis.md` 的精确列/typed DTO和可运行样例。新 `TaskRequest::CourseAnalysis{courseId,previewId,stage}` 使用同一TaskRegistry；`prepare_course_analysis` 返回明确材料范围/费用预留摘要；`start`只接受确认的previewId，不从迁入历史携带批准。

- [ ] 先测试真调用链局部失败fixture、3成功+1失败仅重发第4段、格式/单位错误、取消/切库、过期输入和月度额度耗尽（mock HTTP，不真付费）。Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_analysis -- --test-threads=1`，Expected新任务缺失FAIL，再实现后PASS。
- [ ] 转录先已有字幕，Whisper需工具/模型版本和下载hash通过；模型用户选择安装不默认塞包。选帧/转录/分段/汇总阶段缓存按文件指纹+字幕/帧来源+模型+模板+提示版本隔离。
- [ ] 复用TranslationService的持久预留/结算，新增course用途分项、不清零旧额度；Key只用目标凭据。源码的bounded repair不得继承为新付费许可。历史失效结果仅可浏览。
- [ ] 课程stage的字幕/帧/响应大小在子计划按源fixture单独定限；不全局放宽v0.23资产分析的4帧输入和翻译输出限制，也不静默截断课程全文。超过单请求能力时按完整来源段分批并展示范围。
- [ ] TaskCenter查询活动/失败/需操作，而非仅最近历史；百分比有真实总量才显示，合法内容保留并显示部分完成和定位。Run全回归/构建+模拟服务，Expected 0外部付费请求、无网络自动恢复。Commit `feat(learning): run staged course analysis through shared tasks and budgets`。

M3 门槛：先补精确子计划/固定源修复提交；原用户回执缺失需如实报告，不能把合成修复称为已验证原个案。

## M4：唯一知识库及三态侧栏

### Task 9 / M4A：正文转换、原字节归档与时间引用节点

**Files:** 拟新增 `learning/note_conversion.rs`、`learning/schema-v27.sql`；改 `knowledge.rs`、`knowledge_files.rs`、`note_markdown.rs`、`note_package.rs`、`note_export.rs`、`KnowledgeWorkspace.tsx`；新 `src/lib/learningNoteNodes.tsx`及tests。

**Interfaces（目标明确，源转换字段待固定）:** 新typed节点 `learningReference{courseId,mediaId,fingerprint,ranges:[{startMs,endMs}],label}`、`preservedMarkdown{archiveItemId,label}`；保留节点指向库内不可变原字节段及摘要，不在HTML执行原标记。`convert_source_note(bytes,attachmentMap,mediaMap)->ConversionPreview{document,warnings,unsupported,archiveHash}`、`commit_note_preview(previewId,expectedLibraryId)->NoteId`。新表保存归档项身份/hash/相对路径及来源键，不在当前NoteInput塞createdAt等多余字段。

- [ ] 新fixture覆盖普通可编辑标题/段落/列表/待办/表格/图片/代码/课程链接、BOM/CRLF/frontmatter/wiki等特殊块、1个缺失附件。证明现有转换不足后加保护节点；Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_note -- --test-threads=1`、`npm test -- src/lib/learningNoteNodes.test.tsx`，Expected缺节点FAIL→GREEN。
- [ ] 更新后端节点白名单/搜索/反链、前端渲染/移动/删除确认、复制粘贴/包导入导出/Markdown显式导出；原JSON笔记无迁移重写。前端显示最终排版直接编辑，不保留另一套日常源码编辑器。
- [ ] 归档在learning-data/archives，原字节hash完全一致；转换失败保留pending，不宣称成功；重新导入有目标修订冲突则保留双方，不自动覆盖/写回Vault。未转换历史可浏览/导出并计数。
- [ ] 等源快照格式固定后补转换器子计划与每类支持矩阵。Run全回归/构建/600KiB实际I/O检查，Expected普通格式可编辑、未知块不丢、输入法/undo/粘贴/保存失败可恢复。Commit `feat(knowledge): preserve imported note bytes in the single document model`。

### Task 10 / M4B：三态面板和知识导航

**Files:** 新 `src/lib/workspacePanels.ts`、`src/components/WorkspacePanel.tsx`、tests；改学习工作区、KnowledgeWorkspace/knowledge.css、现有workspace偏好/命令设置。不大改全局素材UI。

**Interfaces:** `PanelState{mode:closed|peek|pinned,width,tab}`、`reducePanel(state,event)`；持久化仅closed/pinned,width/tab，peek不覆盖固定宽度；focus保存布局快照。宽度/快捷键复用现有prefs。

- [ ] 写状态/旧偏好映射/焦点键冲突/IME与对话框外点测试，Run `npm test -- src/lib/workspacePanels.test.ts`，Expected缺能力FAIL→GREEN。
- [ ] 可见窄图标栏、外点/Esc收起peek、同侧单面板、固定拖宽双击复位、退出专注恢复；笔记收件箱/最近/收藏/项目/笔记本导航，侧面板信息合一；保留大纲/思维导图等既有内容能力。
- [ ] 全回归/构建及真实mpv遮挡/125%/150%跨屏验证，Expected布局不重建播放、草稿不丢、按键不抢输入。Commit `feat(workspace): unify learning and knowledge panel behavior`。

## M5：项目学习闭环

### Task 11 / M5：时间码摘录、截图与项目资料

**Files:** 新 `learning/excerpts.rs`、`learning/schema-v28.sql`；改 `knowledge.rs` 的save_in调用、knowledge_files、reference_boards、ProjectHub；新学习摘录预览及测试。

**Interfaces:** `prepare_excerpt(courseId,ranges,subtitleIds,comment,includeFrame)->ExcerptPreview`；`commit_excerpt(expectedLibraryId,operationId,destination,expectedRevision)->ExcerptResult`。destination为已有note/project/board ID；后台持久 operationId 和内容摘要，同ID改内容拒绝。新表 `learning_excerpts`、摘录目标/操作记录、`learning_course_notes`；项目媒体关系继续复用已有表。

- [ ] Task9节点就绪后写字幕原文/非连续range、单帧、无图board拒绝、截图失败文字草稿保留、双击/丢回执/崩溃/磁盘满/归档/切库/目标已编辑测试；Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_excerpt -- --test-threads=1`、`npm test -- src/features/learning/ExcerptComposer.test.tsx`，Expected缺能力FAIL→GREEN。
- [ ] 文件暂存→恢复日志→附件发布→同库事务(save_in/关联)→成功UI；追加只改用户确认位置，不覆盖正文；撤回不能删旧附件。项目信息按真实ID跳转，解除关系不删实体，来源反链可回准确时间。
- [ ] 全回归/构建+真实“项目视频→学习→摘录→参考板/笔记→回看来源”；Expected所有源文件hash不变、操作幂等、缺附件不报告完整成功。Commit `feat(projects): connect learning excerpts notes and reference boards`。

## M6：历史迁入、归档复制及恢复

### Task 12 / M6A：只读源快照、映射和预检

**Files:** 拟新增 `learning/import/{mod,models,preview,apply,recovery}.rs`、`learning/schema-v29.sql`、`src/features/learning/ImportLearningDialog.tsx`及tests。只在正式执行阶段创建。

**依赖：** 等源端确认只读快照/导出manifest schema、逐表字段/内容、source-profile生成规则与样本hash，补 `docs/superpowers/plans/2026-10-21-dcc-learning-migration.md` 后实施。源端readiness已给表名，不等于经过版本化验证的导入格式；不猜测可用正文来自知识FTS缓存。

**已确定Interfaces/invariants:** `preview_learning_import(sourceSelection,targetLibraryId)->ImportPlan` 不写目标；`apply_learning_import(planId,operationId)->TaskId` 进入现有TaskRegistry，新任务LearningImport。`learning_import_batches` / `learning_import_items` / `learning_origin_map` 存(sourceProfileId,kind,sourceKey)唯一、源摘要、目标ID/修订和状态。源旧vault_id+path映射到新UUID，移动文件需要显式重定位；不凭同名/mtime冒继身份。

- [ ] 验证SQLite快照包含WAL、DB+外部Markdown不同时冻结的复核策略；只读模式不执行源迁移/PRAGMA写入、不强杀源程序。草稿/回收/未保存内容独立清单，不自动当正常笔记。
- [ ] 两隔离库fixture：同视频多系列、同名笔记、BOM/CRLF/特殊块、缺附件、历史不可转换、进度冲突、源文件变化、重复执行/中断。Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import -- --test-threads=1`、`npm test -- src/features/learning/ImportLearningDialog.test.tsx`，Expected缺能力FAIL→GREEN。
- [ ] 默认外部原件引用用Task2明确登记；已有相同内容复用媒体但分别保留来源/系列；进度冲突显示双方值让用户选，不取max；摘要/目标修订变化时旧preview失效。费用仅作为映流历史归档，不改目标计数/Key/Cookie。
- [ ] Run全回归/构建、快照二次执行验证新增数为0，Expected源/目标已有内容均保留、失败项目可续作，不触发网络/AI。Commit `feat(migration): preview and resume source-mapped learning imports`。

### Task 13 / M6B：原件归档复制、逐批撤回和备份

**Files:** 扩展 `media_access.rs`、learning/import/recovery、tasks.rs、backup/state，新增归档操作UI及tests。

**Interfaces:** `archive_external_video(expectedLibraryId,entryId,operationId)->TaskId`；`revert_learning_import(batchId,expectedLibraryId)->RevertReport`。复制保留相同media/file/core UUID；本库managed目标ready并hash确认后一次事务移除external登记，grant同时失效；不删除源原件。撤回依赖每项创建/前值/修订/本批新增关系记录。

- [ ] 写低空间、源件替换、取消、复制后DB提交失败、重试已发布文件、撤回前用户编辑、复用旧媒体和附件测试。Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_recovery -- --test-threads=1`，Expected缺能力FAIL→GREEN。
- [ ] copy_atomic/磁盘检查/恢复日志复用，失败保留原引用或明确pending，不产生双份登记。只撤回本批未被后续修改的新对象/关系；冲突留清单不删用户内容。
- [ ] 真实选择样本需单独授权；先生成库完整备份验证再按批执行。全回归+备份/恢复/切库迁移，Expected外部源始终存在、归档后新库可离线播放、不可归档项有明确报告。Commit `feat(migration): safely archive originals and revert unchanged import batches`。

## M7：最终验证与一个安装包

### Task 14 / M7：回归、性能、恢复及交付

**Files:** `docs/evidence/merge/m7.md`、版本兼容/变更说明、完整第三方清单；`scripts/qa/verify-dcc-learning-native.mjs` 和新增 `verify-dcc-learning-migration.mjs`；NSIS仅输出本地release-artifacts。

- [ ] 完整执行项目选视频→播放/字幕→分析→笔记编辑→截图/字幕摘录→参考板/项目→来源回看；逐一核对原件hash、未保存退出及中断恢复。保存真实截图、步骤和失败信息。
- [ ] 对原生焦点/全默认键位/IME/100/125/150%DPI/跨屏/最小窗口做真实测试，记录至少一次2小时连续播放的内存/句柄变化和2小时以上课程分析恢复；付费调用另有明确授权才能执行。
- [ ] 对真实5000课程/1000笔记/600KiB笔记记录硬件、I/O、冷≥5次/热≥20次p50/p95；采用旧性能目标：导航≤1s、首屏≤500ms、长文首次交互≤2s、输入更新p95≤100ms。达不到则报告并优化，不能用生成DB冒充真实文件验收。
- [ ] Run公共回归及 `npm run tauri -- build --bundles nsis`，Expected退出0，许可证/来源/所需源码说明与实际二进制一致，SHA256可复算。未授权不安装/卸载/覆盖旧版，不推送合并发布。未实测范围只能交测试版。
- [ ] 一次全分支最终审阅，修复阻断问题；独立提交本地版本及证据。原映流安装/库/媒体保留；降级使用升级前备份，不让旧程序写新库。

## 公共验证与交付规则

所有测试名称/新模块都是计划新增；首次写出的测试必须先实际失败，再实现后通过。定向命令必须实际运行至少1项预期测试，0项匹配不算通过；模块尚未注册时先按相同interface添加可编译最小错误stub，使测试以行为断言失败，不能将编译/工具错误当RED。所有fixture、grant存储、临时媒体使用隔离路径，禁止测试触碰本机真实用户数据。

每任务最终执行适合的定向检查；新增逻辑任务完成前跑相关全量套件。阶段结束固定：

```powershell
npm test
npm run build
& 'C:/Users/14582/.cargo/bin/cargo.exe' test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1
& 'C:/Users/14582/.cargo/bin/cargo.exe' fmt --all --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
& ./.tools/scan-source-secrets.ps1
```

Expected：退出码0；实际测试数随新增更新，不能固定旧数掩盖跳过。日志放忽略目录 `release-artifacts/yingliu-merge/<phase>/`，提交摘要与hash，不提交真实库/私密字幕/密钥。任何失败注明名称/原因/是否环境问题；只有绿色结果才把阶段标记完成。每阶段独立提交并向当前聊天报告。

## 自审覆盖、待定项及首个任务

- 规格覆盖：设计§1—3→Task2—7/11；§4.1→5—7；§4.2→8；§4.3/6.3→9—10/12；§5→11；§6.1/6.2/6.4/6.5→2—3/8/12—13；§7—9→1/14。Review Focus五类均有具体用例归属。
- 已确认不再询问：唯一主应用、document_json默认权威、保留原文/Vault与显式Markdown导入导出、不做双向Obsidian自动写回。
- 真正阻断：源分析修复的已提交SHA/可复用结果样本尚未固定（阻M3）；mpv实际工具/许可打包选择和完整Windows资源必须核验（阻M2实机/打包）；版本化源快照/导出manifest/稳定source-profile及样本仍须双方按源实况冻结（阻M6）。这些可通过源清单补齐，不需要用户再次选择存储方向。
- 推荐首个实施任务：计划审阅后从Task1冻结输入开始；首个产品改动是Task2“外部视频只读登记和统一文件解析”，先写安全/不删除原件测试，再加增量schema23。未完成Task2—4不先搬播放器UI。
- 执行方式沿用当前会话顺序；本计划到此仅供协调窗口汇总审阅，没有开始产品集成或真实数据迁移。
- 本轮自审/静态验证：14个任务编号连续、M0—M7覆盖、命令无省略占位、未改产品代码；源代码/接口按实际文件核对。源清单与设计未来修订须在Task1重新固定，不将本计划中的拟新增类型误认为已有实现。
