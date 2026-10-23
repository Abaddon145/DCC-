# Task8.1：课程分析离线结果校验

2026-10-23；基线c7c2ba9。本页只记录8.1，不代表Task8整体完成。

## 实现与来源

新增learning/analysis_models.rs、analysis_result.rs、analysis_tests.rs；由现有learning模块注册。纯解析器不读取文件、数据库、凭据，不联网。

参考映流MIT课程DTO及来源核验规则，独立适配当前栈藏TranscriptCue；没有搬入AiService、数据库或后台服务。完整许可YINGLIU-LICENSE.txt已加入打包资源及第三方清单。

只读来源c866508修复、已提交快照0d10cb9；当前读取内容SHA-256：

|来源|SHA-256|
|---|---|
|ai/models.rs|0365F6EEABC62B5EFE82CBB98920463AB9CB1F52D546EFC833DA600BFE839CDA|
|ai/analysis.rs|0FCCE54CE522CCDDCF41106710F8F7EC84CB1D7CB647540A51C27A8931968AFD|
|section-reference-fixture.json|FADDCF711C5A36A37E62E12EB97D0A96409EAB34D039FF6C56F4CA9723241D75|

测试fixture独立合成，不是原用户远程响应。来源fixture标识synthetic=true、observedRemoteResponse=false。

## 可核验行为

- 合法11000ms条目保留；唯一cue-b错配500ms校正40000ms；本机生成诊断，不接受模型伪造sectionValidation。
- 两段分离来源不生成中间时间；未知引用略过；全部来源条目无效且无可用结构时失败，不以概述冒充成功。
- JSON结构、字段、毫秒整数、范围、Unicode字符数和总字节限额分别检查；禁止通过截断“修复”正文。
- 帧来源使用提供的精确时间，不放宽到附近帧。偏移重点略过并警告；此收紧已记录细化计划与ledger。

## 执行结果

日志均在忽略目录release-artifacts/yingliu-merge/，不含密钥或真实付费请求。

- task8-result-red.log：3行为测试在未实现stub处失败；task8-result-green.log：3通过。
- task8-keypoint-red.log：重点全部无效仍空成功，1失败；修正后纳入全量回归。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml`：370通过、0失败、2既有忽略（task8-result-full.log，32.59秒）。四个新测试全部通过。
- `npm run build`：退出0（task8-result-build.log）；已有大chunk提示仍在，不视为失败。
- 前端源码无改动，本阶段未重复前端全测；最近Task7基线342通过，不冒称本阶段重跑。
- fmt、diff及秘密扫描结果随本阶段本地提交记录；发布、安装、真实模型端到端均未执行。

## 未完成

额度用途拆分、确认预览、schema26、分段缓存、统一任务及分析UI仍属8.2—8.5；尚无课程分析联网入口。物理DPI/跨屏/长时验收仍留最终阶段。
