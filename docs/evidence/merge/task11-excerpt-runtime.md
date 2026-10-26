# Task11b1：摘录媒体来源和单帧准备

基线 `84149c9`。复用既有受控媒体读取与FFmpeg；新增后端准备对象和共享单帧函数，没有schema或Tauri命令变更，未开放保存/界面入口。

- 准备对象绑定当前library/root、课程、媒体主文件、完整原件SHA、真实时长、字幕SHA、文字正文，以及可选帧/摘要/警告，不包含源磁盘路径。读取时复用本机外部访问批准及现有只读文件lease。
- 固定参数提取单帧，私人临时目录、-nostdin/-n、30秒超时、1024像素和现有256KiB JPEG限额；取消由现有子进程管理处理。课程分析与摘录共用同一取帧函数。无截图请求不调用FFmpeg。
- 普通解码/取帧失败保留完整文字并明确frame=None/warnings；取消、维护、错库、原件变化仍失败，不能被降为截图警告。返回前再次验证课程/字幕/文件登记和根身份。
- 新测试发现来源主文件可以错绑另一媒体，已在共享查询要求entry_id归属当前视频且role=main，同时保护分析和摘录。
- Windows真实只读lease与库锁线程测试复现等待环。hash、取帧和课程任务取消轮询改用共用try_lock检查，锁忙只延后判断，不当作确认；分析持lease的最终检查锁忙则明确重试，不放宽费用请求前检查。摘录释放lease后才进入最终强作用域核验。

## 验证

- 四项新runtime用例全部stub RED，1.03秒，进程32014退出101；`task11-excerpt-runtime-red.log`。首轮4项GREEN，随后增加字幕、同ID路径移动和跨媒体主文件案例。
- 归属错误真实RED：6通过、1失败，进程85238退出101；`task11-excerpt-runtime-identity-red.log`。共享来源SQL修复后，摘录14项通过2.11秒，原分析真实帧1项通过5.19秒，进程64384退出0。
- 锁环真实RED：有界文件替换线程等1秒仍不能取得写访问，读线程又等库锁；1失败、1.15秒，进程9838退出101；`task11-excerpt-lock-cycle-red.log`。修复后最终摘录 **15项通过**，2.26秒；课程分析、恢复及费用约束 **32项通过**，94.70秒，进程45712退出0；`task11-excerpt-runtime-lock-green.log` / `task11-excerpt-analysis-scope-regression.log`。
- 最终完整Rust **488通过、0失败、2既有忽略**，75.66秒，main/doc0，进程90807退出0；`task11-excerpt-runtime-full-rust-final.log`。先前487项结果在锁修复之前，不作最终依据。
- 前端代码未改，不重复K3的424项前端测试。当前类型及生产前端构建退出0、5.33秒，进程58338退出0；`task11-excerpt-runtime-build-final.log`。既有大chunk、Rust缓存硬链接降级复制及linker提示保留。
- fmt/diff通过；凭据扫描排除Luca/outputs/tmp并在提交前复核。所有视频均为临时合成文件：真实FFmpeg执行不等于真实播放器视口、用户视频、DPI或最终端到端验收。

复杂发布接缝由GPT-6.1 Sol high只读核查，报告落在 `docs/integration/excerpt-publication-seams.md`，不是Task14的最终审查；未用GPT-6 Astra。无源仓库/用户原件/凭据修改，无付费AI、安装、推送、合并或发布。

主Task11未完成。下一步11b2需重新获取原件lease/SHA并以active.try_lock→worker busy_timeout(0)→IMMEDIATE短事务提交，加入不可变预览、操作ID/回执、恢复清单和追加保护；不能将本步未持久化准备对象当作旧授权复用。
