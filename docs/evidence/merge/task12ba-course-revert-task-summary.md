# 课程撤回任务摘要

2026-11-02。课程撤回的完整报告保存在课程批次；新任务记录只保存批次 ID、撤回和保留数量及原媒体保留状态，避免每次任务状态更新时反复写入整份大报告。任务中心继续识别旧格式报告；无效摘要不会显示为成功。

通过 600 项合成系列提交和任务撤回验证摘要数量、任务日志小于 4 KiB、重新加载任务结果及批次完整报告；同时保留报告重试和备份恢复检查。未读取真实映流数据库，未进行桌面手工验收，未合并 main。

后端全量 796 项通过、2 项忽略（`release-artifacts/yingliu-merge/course-revert-task-summary-backend.log`）；前端全量 84 个文件、557 项通过（`release-artifacts/yingliu-merge/course-revert-task-summary-frontend.log`）；前端生产构建和 Rust release 检查通过，仍分别有大包体积提示和编译警告。
