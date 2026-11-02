# Task10c-L：学习区面板接入

基线0e2ea39。接入共享面板，未改原生会话身份、start/stop依赖或课程内容存储；无新schema/依赖。

- 左侧系列、右侧字幕/分析共享closed/peek/pinned行为；同侧互斥，访问后保活，分析输入与滚动不随切tab丢失。保留完成标记、帮助、原课程顺序与离开保存屏障。
- 专注保存两侧快照，退出恢复；布局只改变现有视口几何和遮挡状态。小窗口只CSS夹显示宽度，固定width设置保留，不用重建mpv处理布局。
- 新增独立learning浏览上下文到既有后端白名单，video仍保留，非法模块拒绝。仅相同库/remember启用时保存，保留未知字段；500ms合并并串行保存，卸载flush最终稳定状态。peek/专注临时收起不当作固定偏好保存，迟到恢复不覆盖已操作布局。
- 布局读取/保存错误独立显示并可重新读取，不把它伪装成播放失败；错库不应用或保存偏好。splitter尊重上层已处理的快捷键。

## 验证

- 新学习UI三项RED：旧切tab确实清空采样输入，尚无learning布局保存。后端注册检查RED“未知的浏览模块”，后新增同一服务白名单；`task10-learning-panels-red.log` / `task10-learning-context-red.log`。
- 首轮接入24项通过、1失败：旧系列顺序测试在临时面板仍遮挡时触发播放键。按已批准的overlay优先规则，先确认临时态禁用播放键，再固定系列面板并保留原series-next/非库序断言；未取消快捷键保护。
- splitter在上层preventDefault后仍调宽的新增测试RED，再修复共享入口；`task10-splitter-handled-red.log`。补布局读取失败和其他库返回的重试，播放同一会话保持。
- 最终前端定向2文件28项通过，5.21秒；类型与生产构建退出0、4.33秒，进程12261退出0；`task10-learning-panels-final-targeted.log` / `task10-learning-panels-typecheck.log`。
- 后端注册定向1项通过，进程60569退出0；完整Rust **471通过、0失败、2既有忽略**，128.10秒，main/doc0，进程57450退出0；`task10-learning-panels-full-rust.log`。
- 完整前端 **410项/77文件通过**，49.94秒；生产构建退出0、4.19秒，进程53553退出0；`task10-learning-panels-full-ui.log` / `task10-learning-panels-build.log`。既有大chunk提示保留。
- fmt/diff通过；凭据扫描排除Luca/outputs/tmp，新增证据前460文件0命中，提交前复核。日志位于忽略的release-artifacts/yingliu-merge。

前端测试调用真实React学习区和共享面板，但播放/IPC由合成响应模拟，不等于原生窗口视觉验收。真实mpv、响应式CSS、小窗口、IME及物理DPI/跨屏仍Task14。知识库导航和信息区仍待10c-K；此记录不标主Task10完成。

未访问源用户库、付费AI、推送、合并、发布或覆盖安装。按用户karpathy-guidelines/Ponytail保持实际最小接缝；普通事实用Sol medium，知识库复杂接缝设计使用6.1 Sol high只读子体，未使用GPT-6 Astra。
