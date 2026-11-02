# 课程分析接入事实

2026-10-23，只读核对当前栈藏接口；供主计划Task8编写精确子计划使用，不代表课程分析已实现。调查由GPT-6 Sol medium执行；复杂整合和后续最终审阅按用户模型约束分配，不使用GPT-6 Astra。

- `src-tauri/src/tasks.rs`：TaskRequest现有AIAnalyze；submit先验证并落盘queued，dispatch核对活动库，非下载任务串行。重启将queued/running标为interrupted，不自动网络续作。通用retry复制payload重新提交，课程preview批准必须单独校验，不能因重试继承付费许可。任务列表显示全部活动项、最近200条历史。
- `src-tauri/src/translation_service.rs`：analysis_budget当前限制4张JPEG图片、单图data URL最多350000字符；不能为长课程全局放宽。reserve_for/settle_for使用持久事务计入共用月度usage；creative_usage只区分创作与翻译，没有course用途。CreativeReply保留响应解析失败时的真实used信息；429/5xx可自动重试一次，课程任务须明确自己的重试许可边界。
- `src-tauri/src/ai/analyzer.rs`：PreparedAnalysis含输入preview、库指纹、来源hash、created_at和format_version。load_confirmed_controlled核对24小时TTL、身份、模型、版本及重新hash来源。现有资产缓存及响应字段不适用于课程结果，不覆盖资产表。
- `src-tauri/src/asset/analysis.rs`：运行前验证preview和缓存，使用全局ai::acquire_request租约，取消/切库监控，网络后再验证来源指纹，事务写结果。历史结果可返回但带stale状态。

Task8子计划须具体化独立课程DTO/列、字幕范围和帧来源、分段缓存键、课程用途计数、确认preview及失败阶段续作；沿用主计划的mock HTTP验收，不触发真实付费请求。先固定源c866508结果与sectionValidation fixture，再决定精确映射；本文不以字段猜测代替该步骤。
