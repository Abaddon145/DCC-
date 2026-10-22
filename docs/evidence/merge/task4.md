# Task 4：课程与系列基础

2026-10-22。基线49863cc；schema24增量表learning_courses、learning_series、learning_series_courses，不重建素材/媒体/知识表。

已实现：一个活动视频一个课程，课程标题默认跟随媒体名；多系列有序关联不复制课程。课程以整数毫秒保存位置/时长和progressRevision，运行时进度写入尚未开放，等待Task5后端PlaybackLease。系列更新/删除及完整重排均验证revision，重排在单事务内校验去重/有效课程，错误不留下半份顺序。课程列表分页限制200，系列单次最多10000成员。

命令均带expectedLibraryId，复用AppState scoped连接，错库不回退当前库。前端只有类型和API，无第二套播放器、视频库或导航。删除系列不删课程；软删除媒体使课程不可读取，恢复后身份与关系保留。

RED→GREEN：两项指定store测试先在未实现行为失败，随后通过。补充库范围拒绝及备份恢复保留课程/系列顺序/原asset_core和project_media关联，删除系列后课程保留、重复迁移不重建ID。

最终Rust344通过/2既有忽略/0失败，生产构建成功（既有chunk大小警告）。日志release-artifacts/yingliu-merge/task4-rust.log、task4-build.log；前端316项结果复用Task3（本任务无视图交互改动），秘密扫描357文件/0候选，diff检查通过。新增Progress类型暂未消费的警告会在Task5接入；没有谎报原生播放/DPI验证。

M2输入复核：native_window.rs仍匹配M0 hash，源工作区native_player.rs已变化。继续使用冻结提交或重新核对已提交差异，不将当前未核验字节冒充c866508；这不阻塞已独立验证的目标M1基础。
