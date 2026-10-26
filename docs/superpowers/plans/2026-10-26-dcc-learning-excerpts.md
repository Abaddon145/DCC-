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
