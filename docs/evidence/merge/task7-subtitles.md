# Task7 字幕子阶段检查点

2026-10-23。Task7仍在进行，本提交不是M2整体完成或发布许可。

已接入显式SRT/VTT导入、schema25字幕与cue登记、库内原文件保存、重复内容稳定ID、50段分页、跨页完整Unicode选择和真实时间跳转。学习工作区按需展开字幕面板；原生字幕加载仅接受当前会话的字幕UUID。备份恢复保留原文件、cue和身份；源文件保持原字节。

来源核对由GPT-6 Luna medium只读完成，主线实现顺序进行，未使用GPT-6 Astra。参考映流ai/subtitles.rs解析规则，独立适配栈藏存储与命令；没有迁入旁置字幕扫描、源服务或数据库。源HEAD0d10cb9；本轮来源工作树状态无法可靠读取，因此未据未提交差异移植代码。

字幕导入及面板测试均观察缺能力失败后通过。限制8MiB、每段16000字节、200000段及7天时间范围，拒绝非法范围或编码并明确报错；未截断文本或伪造未知媒体时长。原文件保存使用同目录临时文件、同步及不覆盖发布，事务失败清理本次文件。

验证日志在release-artifacts/yingliu-merge：task7-subtitle-red/green.log、task7-transcript-red/green.log、task7-final-front.log（338通过，69文件）、task7-final-rust.log（364通过、0失败、2既有忽略）、task7-build.log（生产构建退出0）、task7-debug-build.log（调试构建退出0）。全回归首次4项失败均为旧schema24断言，更新至25后复跑通过。秘密扫描386文件/0候选；diff检查通过。

真实隔离目标宿主运行verify-dcc-learning-native.mjs，task7-native-subtitle.log退出0、native=true、postExitVerified=true；实际mpv接受库内登记字幕，收到sub轨道及选中的subtitleId，再执行原有生命周期验收。此前隔离ui-bd90b5进程占用调试exe，经精确路径及参数核实后清理该测试进程，重新构建成功。没有结束用户安装或读取真实库。

待续：课程系列操作界面、完整进度/完成状态交互、原生多音轨与A–B/EOF/继续学习场景、DPI跨屏最终验收。字幕面板完整桌面布局与鼠标选择也仍须真实界面验收；本记录不把组件测试当作桌面通过。新出现的Luca目录未修改或纳入提交。
