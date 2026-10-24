# Task8.6b/c：可选本地转写接入

2026-10-24，基线2024f1d。复用已有素材身份、字幕、TaskRegistry、Windows Job及主题组件；schema仍为26，不增加第二套字幕库或任务队列。

## 功能

- 字幕面板按需打开“本地转写”。只读核验固定版本工具/模型，显示大小、缺失/校验失败及全局本机属性；明确选择资源目录或模型文件才导入，复制固定模型下载链接不发起下载。启动还需勾选本地CPU/临时磁盘确认。
- 新 `CourseTranscription{courseId,modelId}` 要求目标库、UUID和固定模型，重复活动请求复用同任务。可取消、失败后显式重试；关闭面板保留后台任务。成功刷新同一字幕列表，不自动调用在线分析或写笔记。
- 后端固定工具/模型和FFmpeg/FFprobe的9项exe/shared DLL摘要，原件完整hash及只读lease持续到字幕提交；普通任务提交路径阻止切库/维护。抽取完整PCM16/16kHz/单声道，核验后只用CPU识别；中文工作目录采用原生CreateProcessW目录和ASCII相对文件名，不增加shell入口。
- 临时目录在 `.dcc/runtime/transcription`，完成/失败/取消都清理；临时模型优先硬链接，无法链接时用预留空间复制。完整PCM超过RIFF/WAV容量时明确拒绝，不静默剪短；任务执行最长24小时，未知耗时不显示虚构ETA。
- 复用字幕原子文件和SQL事务，耗时文件复制/插入放在worker，最终持当前库短锁再次核对媒体ID、授权、时长及停止状态后提交。数据库失败移除本次字幕文件，原件和旧字幕不变。
- 小型版本化缓存别名保存完整来源/模型/组件/参数指纹与已登记字幕ID/摘要。不同来源得到相同字幕字节时复用同一字幕身份，但各自可复用识别缓存。缓存元数据写失败保留已经完成的有效字幕并报告警告，不把字幕成功伪装成失败。

## 验证

日志位于忽略目录 `release-artifacts/yingliu-merge/`：

- 安装器stub RED→GREEN：只复制固定组件及自身MIT许可，源摘要/头有误不发布，已有损坏目标不覆盖；微型合成模型只进入测试临时配置目录，没有导入真实模型。
- `task8-transcription-cwd-isolated-red.log` / `install-green.log`：目录缺失能力失败后，中文目录与启动前取消通过。最初CMD测试壳引号不适合验证此API，改用仓库已有PowerShell测试模式；仅移除了经精确路径/零字节核对的本次空测试文件。产品Whisper不通过shell执行。
- `task8-transcription-runner-red-permitted.log` / `backend-final-targeted.log`：实际SQLite/媒体lease/TaskRegistry与可控进程边界验证三阶段参数、完整cue入库、缓存、失败/取消/换库/源变化/坏JSON/事务回滚。第一次沙箱拒绝临时库原子保存，普通权限重跑后才计业务RED。
- `task8-transcription-cache-alias-red.log`：来源变化但字幕相同后再次启动仍发进程；缓存别名修正后为0新进程。`final-targeted-v2.log` 后端17项通过，另在完整套件中加入真实受控进程超时检查；已有子树取消/Drop测试继续通过。
- UI空实现RED4→GREEN；`strict-red.log` 捕获StrictMode下陈旧busy锁，修正刷新生命周期；`entry-red.log` 捕获字幕页未提供入口。`final-ui-targeted.log` 10项通过，覆盖明确确认、导入、取消、迟到库响应、字幕回载及非自动安装。
- 首次前端构建发现结果对象的subtitleId为unknown，补明确字符串收窄后：`task8-transcription-final-build.log` 退出0，4.48秒；既有大chunk警告保留。
- 完整 `task8-transcription-runtime-full-rust.log`：418通过、0失败、2既有忽略，123.74秒，main/doc各0，进程退出0。忽略项仍为20k笔记基准及真实Markdown图片网络验收。
- 完整 `task8-transcription-runtime-full-ui.log`：72文件365项通过，46.00秒，退出0。fmt/diff检查通过，凭据扫描421文件0；排除无关Luca、outputs、tmp，未读取其内容。

## 验收边界

真实语音模型尚未选择或安装，未运行真实Whisper识别；模拟过程不是实际音质/中文识别率/长视频性能验收。新AppHandle转写调度与组件导入的实机验证、资源包分发和许可证交付、物理DPI/跨屏等留Task14。Task8.5的原生播放器桌面证据不被改称为这些检查已通过。

未修改映流、原始方案、Vault、真实用户库或原件；没有付费调用、推送、合并、发布或覆盖安装。按用户模型策略只分派一项独立Luna中等推理事实核对，没有Astra或逐任务审查群；子体误认的Whisper许可项已由主会话直接核对JSON纠正为1078字节、94f29b…1d010d，未采用错误值。正式全分支审查仍在所有计划任务之后执行。
