# 学习摘录 UI 接缝事实（Task11c）

2026-10-26，HEAD6d54a0b。前端事实只读核查使用GPT-6 Sol medium；主线程继续实现，未使用Astra。该核查不替代Task14全分支审查，未运行测试或改动界面。

- `TranscriptPanel.tsx` 已有按库/课程key重建、50条分页和跨页完整cue选择，`onSelection`可选；LearningWorkspace当前未传。换字幕/导入/转写会清空选择。seek传cue.startMs。
- `LearningWorkspace.tsx` 已持有受library/course/session校验的PlaybackSnapshot（positionMs/sessionId），并通过control(seek)跳转；右侧字幕/分析panel可作为接入口。当前没有把播放位置传给TranscriptPanel。
- `knowledgeApi(libraryId).search/get/notebooks/referenceTargets` 可查询当前库笔记和笔记本。NoteReferencePicker只选择引用；ProjectPicker/ReferenceBoardPicker会立即添加素材，不能直接作为无副作用的摘录目标选择器。
- `api.listProjects(includeArchived=false)` 和 `api.listReferenceBoards` 可读项目/板选项，但调用时仍须本端作用域核对；不能用全局迟到响应替换切库后选择。
- 原生避让复用可见dialog/modal/backdrop/`data-native-overlay`检测。NativeLearningSurface会发visible=false并串行清理viewport；Workspace同步屏蔽原生快捷键。无需为摘录另建遮挡机制。
- 现有组件复用LoadFeedback、RequestGate/active标记、busy及错误重试。TranscriptPanel.test覆盖跨页选择/seek/迟到页；LearningWorkspace.test mock学习API、通用API、Tauri事件和ResizeObserver。
- TaskRegistry已经有统一submit/get/cancel/retry/恢复中断语义和现有TaskCenter。摘录准备/提交可增加CourseExcerpt payload接入，结果保存UUID/receipt，不将JPEG或完整正文复制进tasks.json。
- 当前TaskRegistry.should_stop通过get克隆整条任务；准备请求可能有数百cue ID及备注，读取大视频时应直接查询停止位以避免每个hash块克隆payload。当前finish会让停止请求覆盖成功状态；摘录提交已经有可靠回执，提交后才来的停止不能显示为未保存。
- prepare_course_analysis虽用spawn_blocking，但自身准备没有独立取消任务。摘录不应另造线程队列；采用既有TaskRegistry准备/提交，关闭弹窗后仍可由任务中心核对和停止。

待实施11c：UUID只读preview/operation查询、统一任务runner、API类型与入口、字幕选择/当前帧→预览确认、重试与恢复。没有据此激活新入口或改变原文/付费调用。

## 导航事实补充

简单定位使用GPT-6 Luna medium，只读、未测试、未改代码（HEAD037d8fb）。`App.tsx`的openLearning(mediaId,projectId?,projectContext?)和openNoteLearning(reference,index,noteId)已使用真实课程/媒体ID及prepareNoteJump指纹。returnFromLearning按returnNoteId/项目上下文/媒体ID恢复来源；LearningWorkspace通过onBack触发退出。指定目标仍由knowledgeInitialId、knowledgeProjectId、referenceInitialBoardId、coreMediaId等现有props交给对应Workspace/Hub。当前LearningWorkspace没有打开已保存笔记/参考板的callback；后续接入口应复用这些状态与安全退出，不另造事件协议。关键位置App.tsx172-192、816-818。
