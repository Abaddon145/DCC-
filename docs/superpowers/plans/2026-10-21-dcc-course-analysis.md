# 栈藏课程分析 Implementation Plan

> **For agentic workers:** 使用executing-plans顺序执行。本文件细化已批准主计划Task8，不重复询问执行方式；总分支完成后一次独立审阅。

**Goal:** 同一栈藏库中提供可核验、可中断续作的课程分析，复用任务与月度预算。

**Architecture:** learning模块维护课程来源快照、分段和汇总结果；TranslationService负责持久预算与HTTP，TaskRegistry负责生命周期。唯一知识库的写入留到Task9—11，不自动生成或覆盖正文。

**Tech Stack:** 当前Rust/rusqlite/Tauri/React/reqwest；不新增数据库服务或任务队列。

**Spec:** 主计划2026-10-21-stackdcc-yingliu-merge.md Task8；只读源设计2026-10-21-stackdcc-yingliu-merge-design.md §4.2/6.4。

## Global Constraints

- 沿用v0.23资产分析、翻译和创作额度，不清零计数。原库、Vault、原件不改动。
- 课程分析结果与asset_ai_analysis分开。所有命令带expectedLibraryId，前端不能传文件路径/模型网址/凭据。
- 每个新付费执行先预览确认；失败阶段重试必须新preview，不能回放旧批准。CI只用本地mock HTTP。
- 优先现有字幕。无字幕时明确本地材料缺失；Whisper工具和模型须单独通过版本/hash/用户安装选择，不能默默下载或虚构转录。
- 源参考c866508的来源校验及0d10cb9已提交快照；只迁纯DTO/算法，保留MIT归属，不搬源AiService。

## Review Focus

1. 非连续字幕之间的虚构时间：只能落在实际引用cue内或精确帧时间。
2. 3段成功第4段失败：保留成功段；重新确认后只请求缺失段。
3. 取消/切库/源替换：请求前后均复核来源，禁止写错库。
4. 超额/退出/响应截断：保守保留预留及真实used，不能以解析失败免计费。
5. 模型伪造sectionValidation及未知JSON字段：诊断由本机生成，结构错误不能降级成功。

## 固定interface与存储

- `CourseAnalysisRequest{courseId,previewId,stage:segments|summary}`；`CourseAnalysisPreview{previewId,courseId,fingerprint,subtitleId?,frameIds,model,templateId,templateVersion,promptVersion,createdAt,expiresAt,stages:[{key,startMs,endMs,cached,tokensMax}],tokensMax,warnings}`。UUID在后端验证；preview24小时到期，开始后一次性消费，网络重试不复用批准。
- 结果DTO使用源JSON字段：contentType、oneSentenceSummary、overview、chapters、keyPoints、tutorialSteps、terms、actionItems、tags、sections、warnings、sectionValidation。时间统一u64毫秒；Chapter为startMs/endMs/title/summary/keywords/keyframeIds，KeyPoint为timeMs/title/content/importance/sourceSegmentIds，TutorialStep为order/timeMs/title/description/notes，Term为name/definition/timeMs；PurposeSection为id/title/items，PurposeItem为title/content/timeMs?/sourceSegmentIds/keyframeIds。SectionIssue记录stage/scopeStartMs/scopeEndMs/sectionId/itemIndex/itemTitle/disposition/reason。
- `parse_course_result(raw,&CourseScope)->Result<CourseAnalysisResult,String>`；Scope含startMs/endMs、完整TranscriptCue及FrameRef{id,timeMs,sha256}。忽略模型sectionValidation，生成本机记录；历史已保存诊断必须另行严格校验，不能作为模型输出再解析而丢掉。
- 分段目标300000ms；完整cue不截断，单请求字幕正文18000字节、帧≤6（JPEG单图350000字符），段≤4096。无法按来源边界安全分段明确失败。JSON响应≤1MiB；全片汇总输入≤120000字节，超限按完整分段结果分组，最多64个经预览批准的汇总请求、层数≤8。不自动发付费修复请求。
- 来源文本上限沿Task7：cue16000字节；结果contentType固定五类，摘要1000字/overview20000字；各结构数组≤200，章节不重叠，步骤order连续；tags≤32×64字，warnings≤8×200字；sections≤12，每栏≤20条，标题80字，条目标题200字/正文2000字，来源≤16cue/6帧，总正文96000字节；本机诊断≤240项。源字符/字节限制保持各自单位。
- 8.1裁定：无字幕来源的重点仅接受精确提供帧时间，不沿用源项目的邻近时间容差；否则无法声称直接定位真实来源。代价是偏移重点会被略过并提示，不猜测纠正。
- schema26：`learning_analysis_previews(id PK,course_id FK,fingerprint,input_json,created_at,expires_at,consumed_at)`；`learning_analyses(id PK,course_id FK,preview_id,source_fingerprint,model,template_id,template_version,prompt_version,format_version,status,result_json,error,created_at,updated_at)`；`learning_analysis_stages(analysis_id FK,stage_key,start_ms,end_ms,input_hash,status,result_json,tokens_used,error,updated_at,PRIMARY KEY(analysis_id,stage_key))`。不可变资料存在learning-data/analysis，以UUID+hash登记；成功段事务提交后可复用，源/模型/模板变化使缓存失效但保留历史。
- 翻译缓存库增量`course_usage(month PRIMARY KEY,used)`；`course_tokens`为独立用途，同时计入原DeepSeek usage，保留旧creative_usage不重分类历史。课程请求使用专用预算函数，不放宽现有analysis_budget四图限制。HTTP只尝试一次；失败后由新preview确认再发送。

### 8.1 离线结果校验

**Files:** 新learning/analysis_models.rs、analysis_result.rs、analysis_tests.rs与合成fixture；mod.rs注册。

- [x] 写行为失败测试：合法项11000ms保留，唯一cue-b错误500ms校正到40000ms并报告；两段非连续引用的25000ms不能校正成中间时间；未知来源略过且全部无有效结果失败；结构/类型/单位/文本超限失败；模型不能注入诊断。
- [x] Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_analysis -- --test-threads=1`；stub行为断言失败，再实现同一纯解析interface并通过；追加全部重点无效测试再次RED→GREEN。
- [x] 校验器不联网/不写库。源fixture为synthetic=true、observedRemoteResponse=false，不称为原用户个案已验证。
- [x] 独立提交087aae5；全Rust370通过/2既有忽略、生产构建0、fmt/diff0、秘密扫描396文件/0。

### 8.2 共用预算用途

**Files:** translation_service.rs、前端TranslationUsage类型及用量显示。

- [x] mock HTTP验证旧创作/翻译计数不变、课程计入总闸口及course_usage、缺usage/取消保留预留、已用尽不发请求、失败不自动发第二次；RED4→GREEN4。
- [x] 增加`course_json_with_usage(body,limit,cancel,image_count)`，内部复用reserve/settle事务和目标凭据；扩展用途typed enum替代内部bool，旧调用适配后行为不变。网络响应有界读取，拒绝过大响应不释放未知费用。
- [x] Run全量回归/生产构建；Rust374+2既有忽略、前端344、构建0；0外部付费请求，本地提交3766f49。

### 8.3 材料预览与缓存

**Files:** learning/analysis.rs、analysis_store.rs、schema-v26.sql；commands.rs/api.ts/types.ts。

资料分段纯函数已完成RED2→GREEN2（证据docs/evidence/merge/task8-segments.md）。完整preview/schema/身份与只读历史已实现；本检查点不暴露付费开始命令，TaskRegistry执行仍属8.4。

- [x] 隔离双库测试preview身份/24h过期/一次消费/字幕或媒体替换/旧缓存/完整cue分段/六图与全文超限，预览不请求HTTP。
- [x] `prepare_course_analysis(expectedLibraryId,courseId,options)`读取受保护媒体lease、完整字幕/允许的帧，生成快照及阶段预算。`get_course_analysis`只返回当前库稳定ID的结果和stale原因；阶段元数据分页，按需读取一个有界结果。
- [x] 原件通过已核验的只读lease及完整hash、字幕前后hash核对；失败不消费批准。一次消费与queued分析/阶段写入同事务，8.4接入实际任务启动；缓存按来源指纹+模型+模板+提示+格式版本隔离。备份恢复撤销旧批准但保留历史快照。
- [x] 定向19项、全Rust385通过/2既有忽略、前端344通过、生产构建0、fmt/diff0、秘密扫描402/0。schema/preview与只读历史保存为本地独立检查点；证据docs/evidence/merge/task8-preview.md。8.4真实任务及8.5面板未完成，不把本记录称为整个Task8完成。

### 8.4 分段及汇总任务

**Files:** tasks.rs、learning/analysis.rs、analysis_store.rs。

分段检查点已实现并验证：真实TaskRegistry日志+localhost HTTP驱动生产runner，3成功/1失败→新preview只发送第4段，全缓存0新请求；391项Rust/345项前端及生产构建通过。证据docs/evidence/merge/task8-runner.md。整课汇总分组/费用预览和桌面实际调度仍未完成，不能把本检查点称为8.4全部完成。

汇总分组检查点：已实现完整结果流式读取/组装、实际转义字节预算、8层/64请求边界、最终合并余量及材料指纹；4项RED→GREEN，完整Rust395通过/2既有忽略、构建0。证据docs/evidence/merge/task8-summary-plan.md。新汇总批准/谱系/执行/历史接口尚未接入，Summary任务仍明确拒绝；不得将离线计划称为已批准或已生成全片汇总。

后续汇总批准/执行检查点：独立24小时快照、一次消费、后端推导的8层/64逻辑阶段谱系、完整祖先/原件复核与缓存已接入同一TaskRegistry。组失败后新确认仅重发缺失组；单个最终组才发布整体结果。399项Rust/345项前端及生产构建通过；证据docs/evidence/merge/task8-summary-approved.md。以上更新取代前一离线检查点的“Summary仍拒绝”状态，但不表示8.5面板或真实桌面验收已完成。64是父链逻辑阶段容量；人工再次确认的失败重试仍按实际用量计入共享月度上限，不是无限自动重试或免计费。

- [x] mock HTTP真调用链：4段中3成功1失败，重新确认preview后仅重发第4段；取消/切库/超额/陈旧preview不误写。汇总超限分组不截断成功段。
- [x] 分段/汇总CourseAnalysis进入TaskRegistry与全局ai::acquire_request；取消监控和网络后再次验源，逐段事务提交。通用retry对课程返回需重新预览，不重放已消费preview；不同类型批准不能互用。
- [x] TaskCenter保留全部活动/失败/需操作项，完成历史200条、逐项结果沿用分页；只有真实阶段总量才显示计数。
- [x] Run全回归及构建，保存本地任务接入检查点；未进行真实AppHandle桌面调度/付费调用，不替代8.5/Task14验收。

### 8.5 学习工作区分析页签

**Files:** CourseAnalysisPanel.tsx及tests、LearningWorkspace、TaskCenter。

- [x] UI测试显示材料范围/费用、确认才提交、部分完成与坏项定位、新preview继续失败阶段、取消及旧库结果忽略；新增长预览每页50项、暂时失败的字幕读取可恢复，25项组件组合测试通过。
- [x] 沿用当前反馈/主题、纯文本输出与原生弹层避让；结果不自动写笔记，保留跳转真实来源入口。真实隔离桌面已核对历史/校验报告、1000ms原生定位、宽/窄窗口、弹层避让与本地汇总预览；未勾选付费确认，退出后课程用量0、原件hash不变。
- [x] 完整前端/Rust/生产构建与本地mock服务、有限真实桌面检查，保存独立本地检查点；Rust400/前端357，构建退出0，证据docs/evidence/merge/task8-analysis-panel.md。付费全链路、DPI/跨屏/长时及原用户故障回执仍不计为已验收。

8.5检查点证据：docs/evidence/merge/task8-analysis-panel.md。完整Rust400通过/2既有忽略；提交前完整前端357通过、生产构建退出0、秘密扫描412文件0。真实桌面只验证本地读取/预览/播放器控制，付费链路仍为本地HTTP runner测试，不合并两类验收。Whisper工具/模型和自动转写尚未接入，不自动下载或制造字幕；本轮以已有字幕及明确采样实现分析闭环，不能据此宣称所有M3/Task14门槛已完成。

自审：覆盖主Task8材料、任务、预算、缓存及部分失败；没有复用资产结果表或扩大资产四图限制。此细化沿用已批准顺序执行；范围内实现选择记入主ledger，不重新请求确认。

### 8.6 可选本地转写（继续已批准 Task8，不扩大产品范围）

**Goal:** 无现成字幕时允许用户选择已核验的本地 Whisper；转写仍进入现有字幕、课程任务和来源校验体系。

**Architecture:** 固定 whisper.cpp 1.9.1 和模型 revision `5359861c739e955e79d9a303bcbc70fb988958b1`，使用 M0 冻结清单，不接收前端可执行文件路径、参数、下载地址或自报摘要。Whisper 放在独立工具目录，模型放在本机配置目录，不混入课程原件或素材库备份。优先已有字幕，不在解析课程或分析时自动安装/执行。

**Spec:** 主计划 Task8、M0 source-map 的 transcription.rs/whisper_models.rs；源代码只读固定 c86650831aecb55ef7bec288df9ae7fb27c8d178，保留现有 YINGLIU-LICENSE.txt。

**Review Focus:** 同大小篡改模型；缺失/替换 DLL；junction/ADS 越权；JSON 时间越界/截断；取消、切库、旧原件与重复结果。前四项由 8.6a 钉住，生命周期由 8.6b/c 钉住。

#### 8.6a 离线工具/模型与结果边界

**Files:** 新 `src-tauri/src/learning/transcription.rs`、`transcription_tests.rs`、`src-tauri/resources/whisper-components.json`；改 `learning/mod.rs`。

**Interfaces:**
- `component_specs() -> Result<WhisperManifest,String>` 只读取编译进程序的固定清单；模型只接受 base/small。
- `verify_components(tool_root:&Path,model_root:&Path,model_id:&str,stop:&dyn Fn()->bool) -> Result<VerifiedTranscription,String>` 仅供后端：每个 exe/DLL 及所选模型完整 SHA256、模型精确长度与 GGML 头、普通文件与祖先无重解析点；返回路径与持有的只读锁，取消不返回成功。版本由固定字节身份确定，不把“文件存在”称为可运行。
- `parse_whisper_json(bytes:&[u8],subtitle_id:&str,duration_ms:u64) -> Result<ParsedTranscript,String>`：JSON ≤32MiB，完整行 ≤200000，每行 ≤16000 UTF-8 字节，整数毫秒、非负、有序、`0 <= start < end <= duration`，时长已知且 ≤7天；空结果失败，不夹断时间、不丢弃坏行。`ParsedTranscript{language:Option<String>,cues:Vec<TranscriptCue>}` 复用既有 cue 身份。
- `transcript_srt(&ParsedTranscript) -> Result<Vec<u8>,String>`：输出完整 SRT ≤8MiB，拒绝会破坏 SRT 分段的空行/控制字符，而不静默修改正文；通过既有字幕解析器验证 round-trip。

- [x] 先写并运行行为失败测试：两段中文/emoji/多行保持原文与毫秒；负数/小数/越界/空/超限/坏行失败；工具缺 DLL、模型同大小篡改、取消、未知ID、junction/ADS 不可通过；锁存在时 Windows 写入被阻止。
- [x] 最小实现；测试使用微型合成清单和文件，不能将其摘要加入产品允许清单；不执行 Whisper、不下载安装模型。
- [x] 定向9项通过；提交前完整 Rust409通过/2既有忽略、前端357通过、生产构建0。证据 `docs/evidence/merge/task8-transcription-boundary.md`，保存独立本地接口检查点；不把它称为真实语音识别已验收。

#### 8.6b 工具选择与同一任务系统

**Files:** 后续细化 `learning/transcription.rs`、`runtime/process.rs`、`learning/commands.rs`、`tasks.rs`、现有准备工具脚本。

- [ ] 用户明确选安装或导入固定模型才落盘；本阶段不后台下载。临时文件、磁盘检查、完整摘要、原子不覆盖发布、失败只清理本次文件。
- [ ] 同一 TaskRegistry 的转写请求只含 courseId/modelId；原件 lease/完整hash、工具模型身份与版本组成缓存键，取消/超时/切库结束 Job 子树，独立缓存目录清理，保留现有成功字幕。
- [ ] 固定 FFmpeg PCM16/16kHz/单声道与 Whisper CPU 参数；兼容中文路径，不开放 shell/cwd/自定义参数。输出经 8.6a 解析和既有字幕事务登记；同任务重试需重新验证原件及组件。
- [ ] 本地 fake process/合成原件验证取消、超时、部分文件、源替换、幂等及字幕回滚；用户选择真实模型后才进行真实识别验收，不混淆两种证据。

#### 8.6c 学习工作区入口

**Files:** 后续细化 `TranscriptPanel.tsx`、learning API/types 及测试。

- [ ] 显示未安装/校验失败/可用与模型大小，明确本地 CPU/磁盘成本；用户启动后进入统一任务中心，完成刷新现有字幕列表，不自动发付费分析。
- [ ] UI 测试覆盖缺组件、明确选择、旧库结果忽略、取消与新字幕可定位；最后统一 Task14 的真实模型、DPI/跨屏验收。

8.6 自审裁定：这是主 Task8 已批准的可选转录 seam；沿用顺序执行，不再询问方案。先离线验证边界，再接实际任务与显式安装入口；当前没有模型，不声称真转写通过，也不默默拉取约148/488MB模型。
