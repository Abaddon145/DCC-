# Task 2：外部视频只读登记

2026-10-22。只改栈藏；未读取真实映流库、未安装或发布。

实现：schema23增量登记，视频预览→确认令牌（库/根/指纹绑定、15分钟、重复确认复用结果），SHA去重、同内容重新定位/修订冲突拒绝、本机批准与访问nonce。MediaFile明确storageMode/sourceRevision，不向前端输出原件路径。实际视频流由现有FFprobe验证；处理在blocking工作线程，预览/确认hash不持有活动库锁。

Windows实际使用只读文件句柄、FILE_SHARE_READ、卷/文件索引及GetFinalPathNameByHandleW；CheckedMediaFile在消费期间持有句柄，禁止替换/写入。读前拒绝UNC/设备/ADS/脚本、junction及祖先重解析；未授权/错库/复制库/源替换拒绝。保留现有托管原件语义，不引入第二个媒体身份或队列。

调用审计：

- media_library::protocol_response按UUID走统一解析；bundle_response仅托管model依赖。
- asset::repository::file_path返回checked lease；metadata/process/retry、asset service附加版本与原件校验、AI analyzer的原件/缩略图读取自动复用同一边界。
- db健康检查按登记文件解析；永久清理显式排除media_external_files，FK只清关系，不删原件。模型缩略图/收录/托管导入路径仍只处理自己发布的media-library路径。
- 外部main使用保留external-references定位符；缺失外部登记不能作为普通托管文件读取。

测试：先观察未实现registration/resolution行为失败，再实现通过；relocation也先失败后通过。实际生成64×64短视频。6项定向覆盖原件hash/mtime不变、写句柄拒绝、重复确认、错库、空批准、同内容重定位/内容变化、过期令牌、Windows实际junction、软删恢复和永久清理、托管读取、伪装播放列表拒绝。

完整Rust回归：337通过、2个既有忽略、0失败；日志release-artifacts/yingliu-merge/task2-rust-green.log。生产前端构建成功（既有大chunk警告）；日志task2-build.log。首次全量333通过/4失败/2忽略：旧22版本断言和两个只跑旧asset迁移的夹具；已明确升级期望和夹具后复测，不放宽生产读取边界。初始沙箱原子保存失败与正常权限下行为RED分别记录。

边界：这不是播放器UI或真实双应用联调；备份恢复后的nonce失效在Task3实现，课程在Task4实现。所有输入/输出仅为隔离夹具。读取仍对外部内容做完整指纹核验；大型原件的锁外解析/缓存优化在播放器会话接入时验证，不把小样本数字当长视频性能结果。
