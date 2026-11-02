# Task10c-K2：知识库左侧面板

基线 `fb9c22b`。将原导航和笔记列表放入同一个共享 WorkspacePanel，保留原有笔记本拖动、列表排序、编辑器及保存队列；没有新依赖、数据库变更或第二套笔记模型。

- 收件箱使用 K1 的 unfiledOnly，项目筛选使用本库真实项目 UUID 和直接引用关系，最近更新沿用后台更新时间排序；计数和分页均由后台过滤。新视图暂禁手动排序，返回全部笔记后恢复原手动偏好。项目选择不修改笔记或自动建立关联。
- 收起、临时展开、固定及专注恢复保留同一个编辑器、草稿、选区、undo、搜索和列表滚动；左侧整体调宽仅改变全局 listWidth，navigationWidth 与颜色等其他外观字段保留。显示夹宽不写回固定宽度。
- 外观配置成功读取前不自动保存宽度；错误可重试。拖宽 400ms 合并并串行写入，离开时 flush。外观弹窗仍实时预览、取消不保存，显式应用复用同一写入序列。
- 同一 knowledge context 保留未知字段、标签、活动笔记和排序。读取失败不写默认布局；迟到布局不覆盖已操作面板，迟到 activeId 或其异步笔记响应不替换用户已选笔记和草稿。仅相同库且记忆启用时保存，peek/专注临时状态沿用共享持久化规则。
- 共享 peek 在 library-pointer-dragging 时把 Escape 和外点留给拖动处理。固定面板使用 flex 收窄，保留原学习区布局行为。

## 验证

- 首轮沙箱测试：知识库 suite 因 sandbox Temp ENOENT 无法加载；共享拖动优先用例真实失败。原代码经许可重跑：**6 失败、35 通过**，分别暴露尚无导航、面板、宽度和迟到恢复保护；日志 `task10-knowledge-left-red-permitted.log`。
- 首轮实现：39 通过、2 失败。一个测试对带 output 的隐式 slider 名称要求过严，改为标签前缀；另一个在 jsdom 中检查 CSS 隐藏元素的可访问性，改为核实 pending 状态和禁止写入的实际行为。没有为测试新增无用组件接口。
- 组合回归曾一次失败于现有 Ctrl+E 连续切换用例；原代码单独复测通过，随后包含该用例的最终定向和完整回归均通过。未据此更改快捷键实现，保留诊断日志 `task10-left-shortcut-diagnostic.log`。
- 最终定向 **65 项/3 文件通过**、19.90 秒，进程50346退出0；`task10-knowledge-left-target-final.log`。包含新增上下文读失败、活动笔记响应迟到、离开时宽度 flush 和导航搜索/滚动保留检查。
- 完整前端 **419 项/77 文件通过**、53.16 秒，进程82977退出0；`task10-knowledge-left-full-ui.log`。后端未改，不重复 Rust 全套；最近 K1 的 Rust 为473通过、2既有忽略。
- 检查发现 LearningReview/NoteRecovery 固定弹窗也位于内部导航树，因此移除内部 container containment，改用已有 flex/max-width 和媒体查询，避免把全局弹窗限制在面板范围。最终类型及生产构建退出0、4.39秒，进程76158退出0；`task10-knowledge-left-final-build.log`。既有大 chunk 提示保留。
- diff 检查通过；凭据扫描排除 Luca/outputs/tmp，证据文件加入前462文件0命中，提交前复核。日志位于忽略的 release-artifacts/yingliu-merge。

测试使用真实 React/Tiptap、模拟 IPC 和合成数据；不等于原生窗口视觉、中文输入法、长文或物理 DPI 验收，这些仍在 Task14。主 Task10 尚未完成：K3 右侧信息区待接入。

未访问或修改映流用户数据、源仓库、付费 AI、用户安装，未推送、合并或发布。按用户 karpathy-guidelines 与 Ponytail 采用现有组件和最小接缝；本阶段无需追加子体，未使用 GPT-6 Astra。
